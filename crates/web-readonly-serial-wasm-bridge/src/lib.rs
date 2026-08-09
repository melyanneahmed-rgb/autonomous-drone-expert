#![forbid(unsafe_code)]

//! Dedicated WebAssembly facade for read-only Web Serial discovery.
//!
//! JavaScript receives only three directive kinds: open the explicitly selected port,
//! exchange one Rust-authorised identification read, and close. It cannot select a command,
//! construct an MSP frame, provide a `WriteApproval`, or obtain a generic transport effect.
//! Raw response chunks return to the bounded Rust MSP accumulator.

use core::fmt;

use ade_core_api::{ScopeStatus, check_scope};
use ade_execution::{ExecError, IdentificationRequest, ReadonlyIdentification};
use ade_facts::DeviceIdentity;
use ade_protocol_msp::{
    CommandId, Direction, MspError, MspV1ResponseAccumulator, ResponseProgress, decode_frame,
};
use ade_runtime_ports::{
    BoundaryError, IoCoordinator, IoEffect, IoResponse, OutboundPacket, RequestId, TransportEffect,
    TransportFailure, TransportResult,
};
use ade_safety::WriteCommandClass;
use ade_session::SessionState;
use wasm_bindgen::prelude::*;

#[derive(Debug)]
enum BridgeError {
    Boundary,
    Execution,
    Protocol,
    InvalidDecimal,
    InvalidFailure,
    InvalidState,
    NonTransportEffect,
    DirectiveRefused,
}

impl fmt::Display for BridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let marker = match self {
            Self::Boundary => "BOUNDARY",
            Self::Execution => "IDENTITY",
            Self::Protocol => "PROTOCOL",
            Self::InvalidDecimal => "INVALID_REQUEST_ID_DECIMAL",
            Self::InvalidFailure => "INVALID_TRANSPORT_FAILURE",
            Self::InvalidState => "INVALID_STATE",
            Self::NonTransportEffect => "NON_TRANSPORT_EFFECT",
            Self::DirectiveRefused => "DIRECTIVE_REFUSED",
        };
        write!(formatter, "RUST_WEB_SERIAL_REFUSAL:{marker}")
    }
}

impl From<BoundaryError> for BridgeError {
    fn from(_error: BoundaryError) -> Self {
        Self::Boundary
    }
}

impl From<ExecError> for BridgeError {
    fn from(_error: ExecError) -> Self {
        Self::Execution
    }
}

impl From<MspError> for BridgeError {
    fn from(_error: MspError) -> Self {
        Self::Protocol
    }
}

fn js_error(error: BridgeError) -> JsError {
    JsError::new(&error.to_string())
}

fn parse_request_id(value: &str) -> Result<RequestId, BridgeError> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(BridgeError::InvalidDecimal);
    }
    value
        .parse::<u64>()
        .map(RequestId::new)
        .map_err(|_| BridgeError::InvalidDecimal)
}

fn parse_failure(value: &str) -> Result<TransportFailure, BridgeError> {
    match value {
        "PortBusy" => Ok(TransportFailure::PortBusy),
        "PermissionDenied" => Ok(TransportFailure::PermissionDenied),
        "Unavailable" | "MissingDriver" => Ok(TransportFailure::MissingDriver),
        "Disconnected" => Ok(TransportFailure::Disconnected),
        "Timeout" => Ok(TransportFailure::Timeout),
        "Cancelled" => Ok(TransportFailure::Cancelled),
        "Unknown" => Ok(TransportFailure::Unknown),
        _ => Err(BridgeError::InvalidFailure),
    }
}

const fn failure_label(failure: TransportFailure) -> &'static str {
    match failure {
        TransportFailure::PortBusy => "PortBusy",
        TransportFailure::PermissionDenied => "PermissionDenied",
        TransportFailure::MissingDriver => "Unavailable",
        TransportFailure::Disconnected => "Disconnected",
        TransportFailure::Timeout => "Timeout",
        TransportFailure::Cancelled => "Cancelled",
        TransportFailure::Unknown => "Unknown",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Ready,
    Opening,
    Exchanging,
    Closing,
    Complete,
}

#[derive(Debug)]
enum FinalOutcome {
    InScope(DeviceIdentity),
    ScopeMismatch {
        identity: DeviceIdentity,
        field: &'static str,
    },
    Failed(&'static str),
}

/// One opaque host operation created by the Rust discovery state machine.
#[wasm_bindgen]
pub struct WasmReadonlySerialDirective {
    request_id: String,
    kind: &'static str,
    bytes: Vec<u8>,
}

#[wasm_bindgen]
impl WasmReadonlySerialDirective {
    #[wasm_bindgen(getter, js_name = requestId)]
    pub fn request_id(&self) -> String {
        self.request_id.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        self.kind.to_owned()
    }

    /// Bytes exist only on an `exchange-identification-read` directive and were already
    /// authorised and framed by Rust.
    #[wasm_bindgen(getter)]
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }
}

fn allowed_identification(command: CommandId) -> bool {
    matches!(
        command,
        CommandId::ApiVersion | CommandId::FcVariant | CommandId::FcVersion | CommandId::BoardInfo
    )
}

fn directive_from(
    effect: IoEffect,
    expected_command: Option<CommandId>,
) -> Result<WasmReadonlySerialDirective, BridgeError> {
    match effect {
        IoEffect::Transport {
            request_id,
            effect: TransportEffect::OpenSelectedReadOnlyPort,
        } if expected_command.is_none() => Ok(WasmReadonlySerialDirective {
            request_id: request_id.get().to_string(),
            kind: "open-selected-read-only-port",
            bytes: Vec::new(),
        }),
        IoEffect::Transport {
            request_id,
            effect: TransportEffect::Close,
        } if expected_command.is_none() => Ok(WasmReadonlySerialDirective {
            request_id: request_id.get().to_string(),
            kind: "close",
            bytes: Vec::new(),
        }),
        IoEffect::Transport {
            request_id,
            effect: TransportEffect::Exchange(packet),
        } => {
            let expected = expected_command.ok_or(BridgeError::DirectiveRefused)?;
            if packet.class() != WriteCommandClass::NoWrite
                || packet.approval().is_some()
                || packet.approved_target().is_some()
                || packet.approved_recovery().is_some()
            {
                return Err(BridgeError::DirectiveRefused);
            }
            let frame = decode_frame(packet.bytes())?;
            let command = frame.known_command().ok_or(BridgeError::DirectiveRefused)?;
            if frame.direction != Direction::Request
                || !allowed_identification(command)
                || command != expected
                || frame.payload_len() != 0
            {
                return Err(BridgeError::DirectiveRefused);
            }
            Ok(WasmReadonlySerialDirective {
                request_id: request_id.get().to_string(),
                kind: "exchange-identification-read",
                bytes: packet.bytes().to_vec(),
            })
        }
        IoEffect::Transport { .. } => Err(BridgeError::DirectiveRefused),
        IoEffect::Storage { .. } => Err(BridgeError::NonTransportEffect),
    }
}

/// Rust-owned read-only discovery state exposed through a narrow WebAssembly ABI.
#[wasm_bindgen]
pub struct WasmReadonlySerialDiscovery {
    coordinator: IoCoordinator,
    identification: ReadonlyIdentification,
    phase: Phase,
    pending_id: Option<RequestId>,
    accumulator: Option<MspV1ResponseAccumulator>,
    outcome: Option<FinalOutcome>,
}

impl WasmReadonlySerialDiscovery {
    fn create() -> Result<Self, BridgeError> {
        Self::create_in_state(SessionState::Identifying)
    }

    fn create_in_state(state: SessionState) -> Result<Self, BridgeError> {
        Ok(Self {
            coordinator: IoCoordinator::new(),
            identification: ReadonlyIdentification::new(state)?,
            phase: Phase::Ready,
            pending_id: None,
            accumulator: None,
            outcome: None,
        })
    }

    fn begin_effect(
        &mut self,
        effect: TransportEffect,
        expected_command: Option<CommandId>,
        next_phase: Phase,
    ) -> Result<WasmReadonlySerialDirective, BridgeError> {
        let effect = self.coordinator.begin_transport(effect)?;
        self.pending_id = Some(effect.request_id());
        self.phase = next_phase;
        directive_from(effect, expected_command)
    }

    fn begin_open(&mut self) -> Result<WasmReadonlySerialDirective, BridgeError> {
        if self.phase != Phase::Ready {
            return Err(BridgeError::InvalidState);
        }
        self.begin_effect(
            TransportEffect::OpenSelectedReadOnlyPort,
            None,
            Phase::Opening,
        )
    }

    fn verify_pending(&self, request_id: &str, phase: Phase) -> Result<RequestId, BridgeError> {
        if self.phase != phase {
            return Err(BridgeError::InvalidState);
        }
        let request_id = parse_request_id(request_id)?;
        if self.pending_id != Some(request_id) {
            return Err(BridgeError::Boundary);
        }
        Ok(request_id)
    }

    fn next_exchange(&mut self) -> Result<WasmReadonlySerialDirective, BridgeError> {
        let request: IdentificationRequest = self.identification.next_request()?;
        let command = request.command();
        let packet = OutboundPacket::read_only(request.bytes().to_vec())?;
        self.accumulator = Some(MspV1ResponseAccumulator::new(command));
        self.begin_effect(
            TransportEffect::Exchange(packet),
            Some(command),
            Phase::Exchanging,
        )
    }

    fn start_close(&mut self) -> Result<WasmReadonlySerialDirective, BridgeError> {
        self.accumulator = None;
        self.begin_effect(TransportEffect::Close, None, Phase::Closing)
    }

    fn accept_open_ok(
        &mut self,
        request_id: &str,
    ) -> Result<WasmReadonlySerialDirective, BridgeError> {
        let request_id = self.verify_pending(request_id, Phase::Opening)?;
        self.coordinator.accept(IoResponse::Transport {
            request_id,
            result: TransportResult::Open(Ok(())),
        })?;
        self.pending_id = None;
        self.next_exchange()
    }

    fn accept_open_err(&mut self, request_id: &str, failure: &str) -> Result<(), BridgeError> {
        let request_id = self.verify_pending(request_id, Phase::Opening)?;
        let failure = parse_failure(failure)?;
        self.coordinator.accept(IoResponse::Transport {
            request_id,
            result: TransportResult::Open(Err(failure)),
        })?;
        self.pending_id = None;
        self.outcome = Some(FinalOutcome::Failed(failure_label(failure)));
        self.phase = Phase::Complete;
        Ok(())
    }

    fn fail_exchange(
        &mut self,
        request_id: RequestId,
        label: &'static str,
    ) -> Result<WasmReadonlySerialDirective, BridgeError> {
        self.coordinator.cancel_transport(request_id)?;
        self.pending_id = None;
        self.outcome = Some(FinalOutcome::Failed(label));
        self.start_close()
    }

    fn accept_chunk(
        &mut self,
        request_id: &str,
        chunk: &[u8],
    ) -> Result<Option<WasmReadonlySerialDirective>, BridgeError> {
        let request_id = self.verify_pending(request_id, Phase::Exchanging)?;
        let progress = match self
            .accumulator
            .as_mut()
            .ok_or(BridgeError::InvalidState)?
            .push(chunk)
        {
            Ok(progress) => progress,
            Err(_) => {
                return self
                    .fail_exchange(request_id, "MalformedResponse")
                    .map(Some);
            }
        };
        let ResponseProgress::Complete(frame) = progress else {
            return Ok(None);
        };

        self.coordinator.accept(IoResponse::Transport {
            request_id,
            result: TransportResult::Exchange(Ok(Vec::new())),
        })?;
        self.pending_id = None;
        self.accumulator = None;
        match self.identification.accept_response(&frame) {
            Ok(Some(identity)) => {
                self.outcome = Some(match check_scope(&identity) {
                    ScopeStatus::InScope => FinalOutcome::InScope(identity),
                    ScopeStatus::Mismatch { field } => {
                        FinalOutcome::ScopeMismatch { identity, field }
                    }
                    ScopeStatus::NotChecked => {
                        return Err(BridgeError::InvalidState);
                    }
                });
                self.start_close().map(Some)
            }
            Ok(None) => self.next_exchange().map(Some),
            Err(_) => {
                self.outcome = Some(FinalOutcome::Failed("ProtocolIdentityFailure"));
                self.start_close().map(Some)
            }
        }
    }

    fn accept_exchange_err(
        &mut self,
        request_id: &str,
        failure: &str,
    ) -> Result<WasmReadonlySerialDirective, BridgeError> {
        let request_id = self.verify_pending(request_id, Phase::Exchanging)?;
        let failure = parse_failure(failure)?;
        self.coordinator.accept(IoResponse::Transport {
            request_id,
            result: TransportResult::Exchange(Err(failure)),
        })?;
        self.pending_id = None;
        self.outcome = Some(FinalOutcome::Failed(failure_label(failure)));
        self.start_close()
    }

    fn accept_close(&mut self, request_id: &str, failure: Option<&str>) -> Result<(), BridgeError> {
        let request_id = self.verify_pending(request_id, Phase::Closing)?;
        let result = match failure {
            Some(value) => TransportResult::Close(Err(parse_failure(value)?)),
            None => TransportResult::Close(Ok(())),
        };
        self.coordinator
            .accept(IoResponse::Transport { request_id, result })?;
        self.pending_id = None;
        if failure.is_some() {
            self.outcome = Some(FinalOutcome::Failed("CloseFailure"));
        }
        self.phase = Phase::Complete;
        Ok(())
    }

    fn identity(&self) -> Option<&DeviceIdentity> {
        match self.outcome.as_ref()? {
            FinalOutcome::InScope(identity) | FinalOutcome::ScopeMismatch { identity, .. } => {
                Some(identity)
            }
            FinalOutcome::Failed(_) => None,
        }
    }
}

#[wasm_bindgen]
impl WasmReadonlySerialDiscovery {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<WasmReadonlySerialDiscovery, JsError> {
        Self::create().map_err(js_error)
    }

    #[wasm_bindgen(js_name = begin)]
    pub fn begin(&mut self) -> Result<WasmReadonlySerialDirective, JsError> {
        self.begin_open().map_err(js_error)
    }

    #[wasm_bindgen(js_name = acceptOpenSuccess)]
    pub fn accept_open_success(
        &mut self,
        request_id: &str,
    ) -> Result<WasmReadonlySerialDirective, JsError> {
        self.accept_open_ok(request_id).map_err(js_error)
    }

    #[wasm_bindgen(js_name = acceptOpenFailure)]
    pub fn accept_open_failure(&mut self, request_id: &str, failure: &str) -> Result<(), JsError> {
        self.accept_open_err(request_id, failure).map_err(js_error)
    }

    #[wasm_bindgen(js_name = acceptReadChunk)]
    pub fn accept_read_chunk(
        &mut self,
        request_id: &str,
        chunk: Vec<u8>,
    ) -> Result<Option<WasmReadonlySerialDirective>, JsError> {
        self.accept_chunk(request_id, &chunk).map_err(js_error)
    }

    #[wasm_bindgen(js_name = acceptExchangeFailure)]
    pub fn accept_exchange_failure(
        &mut self,
        request_id: &str,
        failure: &str,
    ) -> Result<WasmReadonlySerialDirective, JsError> {
        self.accept_exchange_err(request_id, failure)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = acceptCloseSuccess)]
    pub fn accept_close_success(&mut self, request_id: &str) -> Result<(), JsError> {
        self.accept_close(request_id, None).map_err(js_error)
    }

    #[wasm_bindgen(js_name = acceptCloseFailure)]
    pub fn accept_close_failure(&mut self, request_id: &str, failure: &str) -> Result<(), JsError> {
        self.accept_close(request_id, Some(failure))
            .map_err(js_error)
    }

    #[wasm_bindgen(getter, js_name = outcomeKind)]
    pub fn outcome_kind(&self) -> String {
        match &self.outcome {
            Some(FinalOutcome::InScope(_)) if self.phase == Phase::Complete => "in-scope",
            Some(FinalOutcome::ScopeMismatch { .. }) if self.phase == Phase::Complete => {
                "scope-mismatch"
            }
            Some(FinalOutcome::Failed(_)) if self.phase == Phase::Complete => "failed",
            _ => "pending",
        }
        .to_owned()
    }

    #[wasm_bindgen(getter, js_name = failureClass)]
    pub fn failure_class(&self) -> Option<String> {
        match &self.outcome {
            Some(FinalOutcome::Failed(label)) => Some((*label).to_owned()),
            _ => None,
        }
    }

    #[wasm_bindgen(getter, js_name = scopeMismatchField)]
    pub fn scope_mismatch_field(&self) -> Option<String> {
        match &self.outcome {
            Some(FinalOutcome::ScopeMismatch { field, .. }) => Some((*field).to_owned()),
            _ => None,
        }
    }

    #[wasm_bindgen(getter, js_name = apiVersion)]
    pub fn api_version(&self) -> Option<String> {
        self.identity()
            .map(|identity| format!("{}.{}", identity.api.api_major, identity.api.api_minor))
    }

    #[wasm_bindgen(getter, js_name = fcVariant)]
    pub fn fc_variant(&self) -> Option<String> {
        self.identity()
            .map(|identity| identity.variant.identifier_string())
    }

    #[wasm_bindgen(getter, js_name = fcVersion)]
    pub fn fc_version(&self) -> Option<String> {
        self.identity().map(|identity| {
            format!(
                "{}.{}.{}",
                identity.version.major, identity.version.minor, identity.version.patch
            )
        })
    }

    #[wasm_bindgen(getter, js_name = targetName)]
    pub fn target_name(&self) -> Option<String> {
        self.identity().map(|identity| identity.target_name.clone())
    }

    #[wasm_bindgen(getter, js_name = hardwareObserved)]
    pub fn hardware_observed(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ade_protocol_msp::{Direction, encode_frame};
    use ade_safety::{ExecutionTarget, RecoveryClass, authorize_write};

    fn transport_effect(effect: TransportEffect) -> IoEffect {
        IoEffect::Transport {
            request_id: RequestId::new(9),
            effect,
        }
    }

    #[test]
    fn only_the_four_empty_identification_requests_cross_the_facade() {
        for command in [
            CommandId::ApiVersion,
            CommandId::FcVariant,
            CommandId::FcVersion,
            CommandId::BoardInfo,
        ] {
            let bytes = ade_protocol_msp::encode_frame(Direction::Request, command, &[]).unwrap();
            let packet = OutboundPacket::read_only(bytes.clone()).unwrap();
            let directive = directive_from(
                transport_effect(TransportEffect::Exchange(packet)),
                Some(command),
            )
            .unwrap();
            assert_eq!(directive.kind, "exchange-identification-read");
            assert_eq!(directive.bytes, bytes);
        }
    }

    #[test]
    fn refused_identification_contexts_cannot_create_a_web_serial_directive() {
        let all_states = [
            SessionState::Disconnected,
            SessionState::Connecting,
            SessionState::Identifying,
            SessionState::SnapshotRead,
            SessionState::Planning,
            SessionState::AwaitingApproval,
            SessionState::BackingUp,
            SessionState::ApplyingTransient,
            SessionState::TransientWritePendingReconcileOnResume,
            SessionState::Saving,
            SessionState::Rebooting,
            SessionState::Reconnecting,
            SessionState::Verifying,
            SessionState::Recovering,
            SessionState::CompletedVerified,
            SessionState::CompletedRestored,
            SessionState::StateUnknownRecoveryRequired,
        ];

        for state in all_states {
            match WasmReadonlySerialDiscovery::create_in_state(state) {
                Ok(mut discovery) => {
                    assert!(state.permits_identification(), "unexpected state {state:?}");
                    assert_eq!(
                        discovery.begin_open().unwrap().kind,
                        "open-selected-read-only-port",
                    );
                }
                Err(BridgeError::Execution) => {
                    assert!(!state.permits_identification(), "refused state {state:?}");
                }
                Err(error) => panic!("unexpected error for {state:?}: {error:?}"),
            }
        }
    }

    #[test]
    fn prohibited_read_write_unknown_malformed_approval_and_wrong_state_never_form_a_directive() {
        for command in [
            CommandId::BeeperConfig,
            CommandId::SetBeeperConfig,
            CommandId::EepromWrite,
            CommandId::Reboot,
        ] {
            let bytes = encode_frame(Direction::Request, command, &[]).unwrap();
            let effect = match OutboundPacket::read_only(bytes.clone()) {
                Ok(packet) => TransportEffect::Exchange(packet),
                Err(_) => {
                    let approval = authorize_write(
                        ExecutionTarget::Mock,
                        match command {
                            CommandId::SetBeeperConfig => WriteCommandClass::TransientConfig,
                            CommandId::EepromWrite => WriteCommandClass::PersistentConfig,
                            CommandId::Reboot => WriteCommandClass::Reboot,
                            _ => unreachable!(),
                        },
                        match command {
                            CommandId::SetBeeperConfig => {
                                RecoveryClass::TransientWritePendingReconcileOnResume
                            }
                            CommandId::EepromWrite => RecoveryClass::AutomaticRollbackSupported,
                            CommandId::Reboot => RecoveryClass::ManualRecoveryRequired,
                            _ => unreachable!(),
                        },
                    )
                    .unwrap();
                    TransportEffect::Exchange(OutboundPacket::approved(bytes, approval).unwrap())
                }
            };
            assert!(directive_from(transport_effect(effect), Some(CommandId::ApiVersion)).is_err());
        }

        let unknown = vec![b'$', b'M', b'<', 0, 99, 99];
        assert!(OutboundPacket::read_only(unknown).is_err());
        assert!(OutboundPacket::read_only(vec![1, 2, 3]).is_err());

        let allowed = OutboundPacket::read_only(
            encode_frame(Direction::Request, CommandId::ApiVersion, &[]).unwrap(),
        )
        .unwrap();
        assert!(
            directive_from(transport_effect(TransportEffect::Exchange(allowed)), None).is_err()
        );
    }

    #[test]
    fn stale_wrong_kind_and_duplicate_responses_leave_pending_state_honest() {
        let mut bridge = WasmReadonlySerialDiscovery::create().unwrap();
        let open = bridge.begin_open().unwrap();
        assert!(bridge.accept_open_ok("2").is_err());
        assert!(bridge.coordinator.has_pending_transport());
        assert!(
            bridge
                .coordinator
                .accept(IoResponse::Transport {
                    request_id: RequestId::new(1),
                    result: TransportResult::Close(Ok(())),
                })
                .is_err()
        );
        assert!(bridge.coordinator.has_pending_transport());
        let exchange = bridge.accept_open_ok(&open.request_id).unwrap();
        assert!(bridge.accept_open_ok(&open.request_id).is_err());
        assert!(bridge.coordinator.has_pending_transport());
        assert_ne!(exchange.request_id, open.request_id);
    }
}
