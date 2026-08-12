#![forbid(unsafe_code)]

//! # `ade-execution` — the single, non-bypassable outbound execution path (M1)
//!
//! Every outbound command of the lifecycle goes through [`Executor`]. There is no other
//! sanctioned way to reach a transport, and the executor's API is **typed only**: callers
//! choose a [`ReadOperation`] or a [`WriteOperation`] — never a raw frame, raw command byte
//! or arbitrary payload. Payload shapes are therefore correct by construction (all reads and
//! `MSP_EEPROM_WRITE`/`MSP_REBOOT` are empty; the beeper write is exactly four bytes).
//!
//! Before any exchange the executor checks, in order:
//! 1. the current [`SessionState`] permits the command's [`WriteCommandClass`];
//! 2. for a write/reboot: a [`WriteApproval`] is present and its target, class and recovery
//!    class each match the executor's target, the operation's class and the step's declared
//!    recovery class;
//! 3. the executor itself refuses to exist for [`ExecutionTarget::Hardware`], so a hardware
//!    exchange is unrepresentable before any transport is even opened.
//!
//! Every reply is validated strictly: decoded, direction-checked (`!` is a failure, never an
//! ACK), classified by the [`Correlator`] (duplicate/out-of-order/unsolicited replies are
//! typed errors), command-matched against the request, and — for writes — required to carry
//! an empty payload. Raw request/reply payloads are never logged; the transport's audit sink
//! records metadata only.

use ade_facts::DeviceIdentity;
use ade_protocol_msp::{
    ApiVersion, BeeperConfigSnapshot, CommandId, Correlator, Direction, FcVariant, FcVersion,
    Frame, MspError, ReplyClass, SetBeeperConfig, decode_frame, encode_frame,
};
use ade_safety::{
    ExecutionTarget, HARDWARE_WRITE_GATE_NOT_APPROVED, RecoveryClass, WriteApproval,
    WriteCommandClass,
};
use ade_session::SessionState;
use ade_transport::{LogicalTransport, TransportError};

/// A read the lifecycle may perform. Reads carry no payload by definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadOperation {
    /// `MSP_API_VERSION`.
    ApiVersion,
    /// `MSP_FC_VARIANT`.
    FcVariant,
    /// `MSP_FC_VERSION`.
    FcVersion,
    /// `MSP_BOARD_INFO`.
    BoardInfo,
    /// `MSP_BEEPER_CONFIG` (the full nine-byte snapshot).
    BeeperConfig,
}

impl ReadOperation {
    const fn command(self) -> CommandId {
        match self {
            ReadOperation::ApiVersion => CommandId::ApiVersion,
            ReadOperation::FcVariant => CommandId::FcVariant,
            ReadOperation::FcVersion => CommandId::FcVersion,
            ReadOperation::BoardInfo => CommandId::BoardInfo,
            ReadOperation::BeeperConfig => CommandId::BeeperConfig,
        }
    }
}

/// A write/reboot the lifecycle may perform. The payload is fixed by the variant; an
/// arbitrary payload is unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOperation {
    /// The exact four-byte `MSP_SET_BEEPER_CONFIG` carrying only `beeper_off_flags`.
    SetBeeperOffFlags(u32),
    /// `MSP_EEPROM_WRITE` with an empty payload.
    SaveEeprom,
    /// A normal `MSP_REBOOT` with an empty payload.
    Reboot,
}

impl WriteOperation {
    const fn command(self) -> CommandId {
        match self {
            WriteOperation::SetBeeperOffFlags(_) => CommandId::SetBeeperConfig,
            WriteOperation::SaveEeprom => CommandId::EepromWrite,
            WriteOperation::Reboot => CommandId::Reboot,
        }
    }

    /// The write-command class of this operation.
    #[must_use]
    pub const fn class(self) -> WriteCommandClass {
        match self {
            WriteOperation::SetBeeperOffFlags(_) => WriteCommandClass::TransientConfig,
            WriteOperation::SaveEeprom => WriteCommandClass::PersistentConfig,
            WriteOperation::Reboot => WriteCommandClass::Reboot,
        }
    }
}

/// Why an execution step failed. Structural only — never raw payload content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecError {
    /// The executor refuses to exist for the hardware target. Carries the stable marker.
    HardwareRefused(&'static str),
    /// The current session state does not permit this command class.
    NotPermittedInState {
        /// The state the session was in.
        state: SessionState,
        /// The class that was refused.
        class: WriteCommandClass,
    },
    /// The canonical identity sequence cannot start in the current lifecycle context.
    IdentificationNotPermittedInState {
        /// The state in which identification was refused.
        state: SessionState,
    },
    /// The approval was granted for a different execution target.
    ApprovalTargetMismatch,
    /// The approval was granted for a different write class.
    ApprovalClassMismatch,
    /// The approval's recovery class does not match the step's declared recovery class.
    ApprovalRecoveryMismatch,
    /// The transport failed.
    Transport(TransportError),
    /// A frame or typed payload failed to decode.
    Payload(MspError),
    /// The device answered with an MSP error frame — a command failure, never an ACK.
    ErrorReply {
        /// The command that failed.
        command: u8,
    },
    /// The reply did not match the outstanding request (duplicate/out-of-order/unsolicited).
    ReplyMisclassified(ReplyClass),
    /// The reply carried a different command than the request.
    ReplyCommandMismatch {
        /// The command that was requested.
        expected: u8,
        /// The command the reply carried.
        got: u8,
    },
    /// The reply direction was a request — structurally invalid for a reply.
    ReplyDirectionInvalid,
    /// A write ACK carried a non-empty payload, violating the M1 contract.
    WriteAckNotEmpty {
        /// The command.
        command: u8,
        /// The unexpected payload length.
        len: usize,
    },
    /// An identification request is already awaiting its response.
    IdentificationRequestPending,
    /// A response arrived without an identification request in flight.
    NoIdentificationRequestPending,
    /// The four-command identification sequence has already completed.
    IdentificationAlreadyComplete,
}

impl From<TransportError> for ExecError {
    fn from(error: TransportError) -> Self {
        ExecError::Transport(error)
    }
}

impl From<MspError> for ExecError {
    fn from(error: MspError) -> Self {
        ExecError::Payload(error)
    }
}

/// One Rust-authorised request from the canonical four-command identification sequence.
///
/// The command and frame are created together in Rust. Callers may transmit the bytes but
/// cannot select or replace the command through this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentificationRequest {
    command: CommandId,
    bytes: Vec<u8>,
}

impl IdentificationRequest {
    /// The exact command fixed by the current Rust identification state.
    #[must_use]
    pub const fn command(&self) -> CommandId {
        self.command
    }

    /// The already-framed MSPv1 request bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Current step in the fixed four-read identity sequence.
///
/// This is observation-only state. It exposes neither a command constructor nor a way to
/// advance or replace the Rust-owned sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentificationStage {
    /// Waiting for `MSP_API_VERSION`.
    ApiVersion,
    /// Waiting for `MSP_FC_VARIANT`.
    FcVariant,
    /// Waiting for `MSP_FC_VERSION`.
    FcVersion,
    /// Waiting for `MSP_BOARD_INFO`.
    BoardInfo,
    /// All four typed replies were accepted.
    Complete,
}

impl IdentificationStage {
    const fn command(self) -> Option<CommandId> {
        match self {
            Self::ApiVersion => Some(CommandId::ApiVersion),
            Self::FcVariant => Some(CommandId::FcVariant),
            Self::FcVersion => Some(CommandId::FcVersion),
            Self::BoardInfo => Some(CommandId::BoardInfo),
            Self::Complete => None,
        }
    }
}

/// Incremental form of the existing canonical identification implementation.
///
/// Native Mock/Replay and the browser effect bridge both use this exact state machine. It
/// owns the four-command order, strict response correlation, typed payload decoders and
/// `DeviceIdentity` construction. It accepts no approval and has no write operation.
#[derive(Debug)]
pub struct ReadonlyIdentification {
    stage: IdentificationStage,
    request_pending: bool,
    correlator: Correlator,
    api: Option<ApiVersion>,
    variant: Option<FcVariant>,
    version: Option<FcVersion>,
}

impl ReadonlyIdentification {
    /// Create the canonical sequence in an explicit identity context.
    ///
    /// # Errors
    /// Refuses every state except `Identifying`, `Verifying`, and `Recovering`.
    pub fn new(state: SessionState) -> Result<Self, ExecError> {
        if !state.permits_identification() {
            return Err(ExecError::IdentificationNotPermittedInState { state });
        }
        Ok(Self {
            stage: IdentificationStage::ApiVersion,
            request_pending: false,
            correlator: Correlator::new(),
            api: None,
            variant: None,
            version: None,
        })
    }

    /// Emit the one request permitted by the current identification state.
    ///
    /// # Errors
    /// Refuses a second in-flight request or use after completion.
    pub fn next_request(&mut self) -> Result<IdentificationRequest, ExecError> {
        if self.request_pending {
            return Err(ExecError::IdentificationRequestPending);
        }
        let command = self
            .stage
            .command()
            .ok_or(ExecError::IdentificationAlreadyComplete)?;
        let bytes = encode_frame(Direction::Request, command, &[])?;
        self.correlator.on_request(command);
        self.request_pending = true;
        Ok(IdentificationRequest { command, bytes })
    }

    /// Observe the current typed identity stage for bounded failure diagnostics.
    ///
    /// The returned enum carries no command bytes, payload, device identity or transition
    /// authority.
    #[must_use]
    pub const fn stage(&self) -> IdentificationStage {
        self.stage
    }

    /// Accept exactly one complete response and advance the canonical identity state.
    ///
    /// Returns the typed identity only after `BOARD_INFO`; earlier successful responses
    /// return `None`.
    ///
    /// # Errors
    /// Refuses missing/out-of-order/duplicate/wrong-command/error-direction responses or any
    /// typed payload decode failure.
    pub fn accept_response(&mut self, frame: &Frame) -> Result<Option<DeviceIdentity>, ExecError> {
        if !self.request_pending {
            return Err(ExecError::NoIdentificationRequestPending);
        }
        if matches!(frame.direction, Direction::Request) {
            return Err(ExecError::ReplyDirectionInvalid);
        }
        let expected = self
            .stage
            .command()
            .ok_or(ExecError::IdentificationAlreadyComplete)?;
        let reply_command = frame
            .known_command()
            .ok_or(ExecError::ReplyCommandMismatch {
                expected: expected.as_u8(),
                got: frame.command,
            })?;
        match self.correlator.on_reply(reply_command) {
            ReplyClass::Expected => {}
            other => return Err(ExecError::ReplyMisclassified(other)),
        }
        if frame.command != expected.as_u8() {
            return Err(ExecError::ReplyCommandMismatch {
                expected: expected.as_u8(),
                got: frame.command,
            });
        }
        if matches!(frame.direction, Direction::Error) {
            return Err(ExecError::ErrorReply {
                command: frame.command,
            });
        }

        self.request_pending = false;
        match self.stage {
            IdentificationStage::ApiVersion => {
                self.api = Some(ApiVersion::from_reply(frame)?);
                self.stage = IdentificationStage::FcVariant;
                Ok(None)
            }
            IdentificationStage::FcVariant => {
                self.variant = Some(FcVariant::from_reply(frame)?);
                self.stage = IdentificationStage::FcVersion;
                Ok(None)
            }
            IdentificationStage::FcVersion => {
                self.version = Some(FcVersion::from_reply(frame)?);
                self.stage = IdentificationStage::BoardInfo;
                Ok(None)
            }
            IdentificationStage::BoardInfo => {
                let board = ade_protocol_msp::BoardInfo::from_reply(frame)?;
                self.stage = IdentificationStage::Complete;
                Ok(Some(DeviceIdentity::from_parts(
                    self.api
                        .take()
                        .ok_or(ExecError::NoIdentificationRequestPending)?,
                    self.variant
                        .take()
                        .ok_or(ExecError::NoIdentificationRequestPending)?,
                    self.version
                        .take()
                        .ok_or(ExecError::NoIdentificationRequestPending)?,
                    &board,
                )))
            }
            IdentificationStage::Complete => Err(ExecError::IdentificationAlreadyComplete),
        }
    }

    /// Whether all four responses have been accepted.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self.stage, IdentificationStage::Complete)
    }
}

/// The single execution path. Constructible only for a simulation target.
#[derive(Debug)]
pub struct Executor {
    target: ExecutionTarget,
    correlator: Correlator,
}

impl Executor {
    /// Build an executor for a **simulation** target. The hardware target is refused here,
    /// before any transport is opened — a hardware exchange is unrepresentable.
    ///
    /// # Errors
    /// [`ExecError::HardwareRefused`] for [`ExecutionTarget::Hardware`].
    pub fn new_simulation(target: ExecutionTarget) -> Result<Self, ExecError> {
        if matches!(target, ExecutionTarget::Hardware) {
            return Err(ExecError::HardwareRefused(HARDWARE_WRITE_GATE_NOT_APPROVED));
        }
        Ok(Self {
            target,
            correlator: Correlator::new(),
        })
    }

    /// The simulation target this executor is bound to.
    #[must_use]
    pub const fn target(&self) -> ExecutionTarget {
        self.target
    }

    /// The one strict exchange path: encode, correlate, exchange, decode, classify, match.
    ///
    /// On any failure the correlator is reset: a failed exchange leaves no valid outstanding
    /// request behind, and a stale reply arriving later still classifies as a refusal
    /// (`Unsolicited`) — classification stays fail-closed, it never silently passes.
    fn exchange<T: LogicalTransport>(
        &mut self,
        transport: &mut T,
        command: CommandId,
        payload: &[u8],
    ) -> Result<Frame, ExecError> {
        let result = self.exchange_inner(transport, command, payload, None);
        if result.is_err() {
            self.correlator = Correlator::new();
        }
        result
    }

    fn exchange_with_approval<T: LogicalTransport>(
        &mut self,
        transport: &mut T,
        command: CommandId,
        payload: &[u8],
        approval: &WriteApproval,
        declared_recovery: RecoveryClass,
    ) -> Result<Frame, ExecError> {
        let result = self.exchange_inner(
            transport,
            command,
            payload,
            Some((approval, declared_recovery)),
        );
        if result.is_err() {
            self.correlator = Correlator::new();
        }
        result
    }

    fn exchange_inner<T: LogicalTransport>(
        &mut self,
        transport: &mut T,
        command: CommandId,
        payload: &[u8],
        approval: Option<(&WriteApproval, RecoveryClass)>,
    ) -> Result<Frame, ExecError> {
        let request = encode_frame(Direction::Request, command, payload)?;
        self.correlator.on_request(command);
        let reply_bytes = match approval {
            Some((approval, declared_recovery)) => {
                transport.exchange_with_approval(&request, approval, declared_recovery)?
            }
            None => transport.exchange(&request)?,
        };
        let frame = decode_frame(&reply_bytes)?;
        if matches!(frame.direction, Direction::Request) {
            return Err(ExecError::ReplyDirectionInvalid);
        }
        let Some(reply_command) = frame.known_command() else {
            return Err(ExecError::ReplyCommandMismatch {
                expected: command.as_u8(),
                got: frame.command,
            });
        };
        match self.correlator.on_reply(reply_command) {
            ReplyClass::Expected => {}
            other => return Err(ExecError::ReplyMisclassified(other)),
        }
        if frame.command != command.as_u8() {
            return Err(ExecError::ReplyCommandMismatch {
                expected: command.as_u8(),
                got: frame.command,
            });
        }
        if matches!(frame.direction, Direction::Error) {
            return Err(ExecError::ErrorReply {
                command: frame.command,
            });
        }
        Ok(frame)
    }

    /// Perform one read. The session state must permit `NoWrite` I/O.
    ///
    /// # Errors
    /// Any pre-exchange authority violation or exchange/validation failure.
    pub fn read<T: LogicalTransport>(
        &mut self,
        transport: &mut T,
        state: SessionState,
        operation: ReadOperation,
    ) -> Result<Frame, ExecError> {
        let class = WriteCommandClass::NoWrite;
        if !state.permits_command_class(class) {
            return Err(ExecError::NotPermittedInState { state, class });
        }
        self.exchange(transport, operation.command(), &[])
    }

    /// Perform one write/reboot under an explicit [`WriteApproval`].
    ///
    /// # Errors
    /// Any pre-exchange authority violation (state, approval target/class/recovery) or any
    /// exchange/validation failure. A non-empty ACK payload is a failure.
    pub fn write<T: LogicalTransport>(
        &mut self,
        transport: &mut T,
        state: SessionState,
        operation: WriteOperation,
        approval: &WriteApproval,
        declared_recovery: RecoveryClass,
    ) -> Result<(), ExecError> {
        let class = operation.class();
        if !state.permits_command_class(class) {
            return Err(ExecError::NotPermittedInState { state, class });
        }
        if approval.target() != self.target {
            return Err(ExecError::ApprovalTargetMismatch);
        }
        if approval.class() != class {
            return Err(ExecError::ApprovalClassMismatch);
        }
        if approval.recovery() != declared_recovery {
            return Err(ExecError::ApprovalRecoveryMismatch);
        }
        let payload: Vec<u8> = match operation {
            WriteOperation::SetBeeperOffFlags(flags) => {
                SetBeeperConfig::new(flags).payload().to_vec()
            }
            WriteOperation::SaveEeprom | WriteOperation::Reboot => Vec::new(),
        };
        let frame = self.exchange_with_approval(
            transport,
            operation.command(),
            &payload,
            approval,
            declared_recovery,
        )?;
        if frame.payload_len() != 0 {
            return Err(ExecError::WriteAckNotEmpty {
                command: frame.command,
                len: frame.payload_len(),
            });
        }
        Ok(())
    }

    /// Run the pinned identification sequence (`API_VERSION`, `FC_VARIANT`, `FC_VERSION`,
    /// `BOARD_INFO`) and assemble the composite [`DeviceIdentity`].
    ///
    /// # Errors
    /// Any read or typed-decode failure.
    pub fn identify<T: LogicalTransport>(
        &mut self,
        transport: &mut T,
        state: SessionState,
    ) -> Result<DeviceIdentity, ExecError> {
        let mut identification = ReadonlyIdentification::new(state)?;
        loop {
            let request = identification.next_request()?;
            let reply = decode_frame(&transport.exchange(request.bytes())?)?;
            if let Some(identity) = identification.accept_response(&reply)? {
                return Ok(identity);
            }
        }
    }

    /// Read the full nine-byte beeper snapshot.
    ///
    /// # Errors
    /// Any read or typed-decode failure.
    pub fn read_snapshot<T: LogicalTransport>(
        &mut self,
        transport: &mut T,
        state: SessionState,
    ) -> Result<BeeperConfigSnapshot, ExecError> {
        Ok(BeeperConfigSnapshot::from_reply(&self.read(
            transport,
            state,
            ReadOperation::BeeperConfig,
        )?)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ade_mock_fc::MockFc;
    use ade_safety::authorize_write;
    use ade_transport::{FrameResponder, InMemoryAudit, MockTransport};

    fn snapshot() -> BeeperConfigSnapshot {
        BeeperConfigSnapshot {
            beeper_off_flags: 0,
            dshot_beacon_tone: 1,
            dshot_beacon_off_flags: 2,
        }
    }

    /// A transport that records every outbound request and never expects to be reached in
    /// authority-violation tests.
    struct ProbeTransport {
        requests: Vec<Vec<u8>>,
        reply: Vec<u8>,
    }

    impl ProbeTransport {
        fn replying(reply: Vec<u8>) -> Self {
            Self {
                requests: Vec::new(),
                reply,
            }
        }
    }

    impl LogicalTransport for ProbeTransport {
        fn open(&mut self) -> Result<(), TransportError> {
            Ok(())
        }
        fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
            self.requests.push(request.to_vec());
            Ok(self.reply.clone())
        }
        fn exchange_with_approval(
            &mut self,
            request: &[u8],
            _approval: &WriteApproval,
            _declared_recovery: RecoveryClass,
        ) -> Result<Vec<u8>, TransportError> {
            self.requests.push(request.to_vec());
            Ok(self.reply.clone())
        }
        fn close(&mut self) {}
    }

    fn mock_transport() -> MockTransport<MockFc, InMemoryAudit> {
        let mut t = MockTransport::new(MockFc::new(snapshot()), InMemoryAudit::new());
        t.open().unwrap();
        t
    }

    #[test]
    fn the_executor_refuses_the_hardware_target_before_any_transport_exists() {
        assert_eq!(
            Executor::new_simulation(ExecutionTarget::Hardware).unwrap_err(),
            ExecError::HardwareRefused(HARDWARE_WRITE_GATE_NOT_APPROVED),
        );
    }

    #[test]
    fn a_read_outside_the_authority_matrix_never_reaches_the_transport() {
        let mut executor = Executor::new_simulation(ExecutionTarget::Mock).unwrap();
        let mut probe = ProbeTransport::replying(Vec::new());
        // Planning permits no device I/O at all.
        let result = executor.read(
            &mut probe,
            SessionState::Planning,
            ReadOperation::BeeperConfig,
        );
        assert_eq!(
            result.unwrap_err(),
            ExecError::NotPermittedInState {
                state: SessionState::Planning,
                class: WriteCommandClass::NoWrite,
            },
        );
        assert!(probe.requests.is_empty(), "no frame may be sent");
    }

    #[test]
    fn a_write_without_a_matching_approval_never_reaches_the_transport() {
        let mut executor = Executor::new_simulation(ExecutionTarget::Mock).unwrap();
        let mut probe = ProbeTransport::replying(Vec::new());
        let transient = authorize_write(
            ExecutionTarget::Mock,
            WriteCommandClass::TransientConfig,
            RecoveryClass::TransientWritePendingReconcileOnResume,
        )
        .unwrap();
        // Class mismatch: transient approval used for a persistent save.
        assert_eq!(
            executor
                .write(
                    &mut probe,
                    SessionState::Saving,
                    WriteOperation::SaveEeprom,
                    &transient,
                    RecoveryClass::AutomaticRollbackSupported,
                )
                .unwrap_err(),
            ExecError::ApprovalClassMismatch,
        );
        // Recovery mismatch: right class, wrong declared recovery.
        let persistent = authorize_write(
            ExecutionTarget::Mock,
            WriteCommandClass::PersistentConfig,
            RecoveryClass::AutomaticRollbackSupported,
        )
        .unwrap();
        assert_eq!(
            executor
                .write(
                    &mut probe,
                    SessionState::Saving,
                    WriteOperation::SaveEeprom,
                    &persistent,
                    RecoveryClass::RestoreFromBackupSupported,
                )
                .unwrap_err(),
            ExecError::ApprovalRecoveryMismatch,
        );
        // Target mismatch: approval for Replay used on a Mock executor.
        let replay_approval = authorize_write(
            ExecutionTarget::Replay,
            WriteCommandClass::PersistentConfig,
            RecoveryClass::AutomaticRollbackSupported,
        )
        .unwrap();
        assert_eq!(
            executor
                .write(
                    &mut probe,
                    SessionState::Saving,
                    WriteOperation::SaveEeprom,
                    &replay_approval,
                    RecoveryClass::AutomaticRollbackSupported,
                )
                .unwrap_err(),
            ExecError::ApprovalTargetMismatch,
        );
        // State mismatch: a save is not permitted while identifying.
        assert!(matches!(
            executor
                .write(
                    &mut probe,
                    SessionState::Identifying,
                    WriteOperation::SaveEeprom,
                    &persistent,
                    RecoveryClass::AutomaticRollbackSupported,
                )
                .unwrap_err(),
            ExecError::NotPermittedInState { .. },
        ));
        assert!(probe.requests.is_empty(), "no frame may be sent");
    }

    #[test]
    fn the_set_write_is_exactly_four_payload_bytes_and_reads_are_empty() {
        let mut executor = Executor::new_simulation(ExecutionTarget::Mock).unwrap();
        // Capture the outbound bytes with a probe that answers with a valid empty ACK.
        let ack = encode_frame(Direction::Reply, CommandId::SetBeeperConfig, &[]).unwrap();
        let mut probe = ProbeTransport::replying(ack);
        let approval = authorize_write(
            ExecutionTarget::Mock,
            WriteCommandClass::TransientConfig,
            RecoveryClass::TransientWritePendingReconcileOnResume,
        )
        .unwrap();
        executor
            .write(
                &mut probe,
                SessionState::ApplyingTransient,
                WriteOperation::SetBeeperOffFlags(0x0001_0000),
                &approval,
                RecoveryClass::TransientWritePendingReconcileOnResume,
            )
            .unwrap();
        let sent = decode_frame(&probe.requests[0]).unwrap();
        assert_eq!(
            sent.payload_len(),
            4,
            "the SET payload is exactly four bytes"
        );
        // Reads are empty.
        let reply = encode_frame(Direction::Reply, CommandId::ApiVersion, &[0, 1, 46]).unwrap();
        let mut probe = ProbeTransport::replying(reply);
        let mut executor = Executor::new_simulation(ExecutionTarget::Mock).unwrap();
        executor
            .read(
                &mut probe,
                SessionState::Identifying,
                ReadOperation::ApiVersion,
            )
            .unwrap();
        let sent = decode_frame(&probe.requests[0]).unwrap();
        assert_eq!(sent.payload_len(), 0, "identification reads are empty");
    }

    #[test]
    fn an_error_reply_is_a_failure_never_an_ack() {
        let mut executor = Executor::new_simulation(ExecutionTarget::Mock).unwrap();
        // Arm the mock so the EEPROM write is refused with an MSP error frame.
        let mut armed = MockTransport::new(
            {
                let mut fc = MockFc::new(snapshot());
                fc.set_armed(true);
                fc
            },
            InMemoryAudit::new(),
        );
        armed.open().unwrap();
        armed.enter_operational();
        let approval = authorize_write(
            ExecutionTarget::Mock,
            WriteCommandClass::PersistentConfig,
            RecoveryClass::AutomaticRollbackSupported,
        )
        .unwrap();
        assert_eq!(
            executor
                .write(
                    &mut armed,
                    SessionState::Saving,
                    WriteOperation::SaveEeprom,
                    &approval,
                    RecoveryClass::AutomaticRollbackSupported,
                )
                .unwrap_err(),
            ExecError::ErrorReply {
                command: CommandId::EepromWrite.as_u8(),
            },
        );
    }

    #[test]
    fn duplicate_and_unsolicited_replies_are_typed_failures() {
        // A responder that replies to the second request with a repeat of the first reply.
        struct RepeatsFirstReply {
            fc: MockFc,
            first: Option<Vec<u8>>,
            calls: usize,
        }
        impl FrameResponder for RepeatsFirstReply {
            fn respond(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
                self.calls += 1;
                if self.calls == 2 {
                    return Ok(self.first.clone().expect("first reply recorded"));
                }
                let reply = self.fc.respond(request)?;
                if self.calls == 1 {
                    self.first = Some(reply.clone());
                }
                Ok(reply)
            }
        }
        let mut t = MockTransport::new(
            RepeatsFirstReply {
                fc: MockFc::new(snapshot()),
                first: None,
                calls: 0,
            },
            InMemoryAudit::new(),
        );
        t.open().unwrap();
        let mut executor = Executor::new_simulation(ExecutionTarget::Mock).unwrap();
        // First read completes; the second request receives the first reply again.
        executor
            .read(&mut t, SessionState::Identifying, ReadOperation::ApiVersion)
            .unwrap();
        assert_eq!(
            executor
                .read(&mut t, SessionState::Identifying, ReadOperation::FcVariant)
                .unwrap_err(),
            ExecError::ReplyMisclassified(ReplyClass::Duplicate),
        );

        // Unsolicited: a reply for a command that was never requested.
        let unsolicited = encode_frame(Direction::Reply, CommandId::EepromWrite, &[]).unwrap();
        let mut probe = ProbeTransport::replying(unsolicited);
        let mut executor = Executor::new_simulation(ExecutionTarget::Mock).unwrap();
        assert_eq!(
            executor
                .read(
                    &mut probe,
                    SessionState::Identifying,
                    ReadOperation::ApiVersion,
                )
                .unwrap_err(),
            ExecError::ReplyMisclassified(ReplyClass::Unsolicited),
        );
    }

    #[test]
    fn a_write_ack_with_a_payload_is_refused() {
        let bad_ack = encode_frame(Direction::Reply, CommandId::EepromWrite, &[1]).unwrap();
        let mut probe = ProbeTransport::replying(bad_ack);
        let mut executor = Executor::new_simulation(ExecutionTarget::Mock).unwrap();
        let approval = authorize_write(
            ExecutionTarget::Mock,
            WriteCommandClass::PersistentConfig,
            RecoveryClass::AutomaticRollbackSupported,
        )
        .unwrap();
        assert_eq!(
            executor
                .write(
                    &mut probe,
                    SessionState::Saving,
                    WriteOperation::SaveEeprom,
                    &approval,
                    RecoveryClass::AutomaticRollbackSupported,
                )
                .unwrap_err(),
            ExecError::WriteAckNotEmpty {
                command: CommandId::EepromWrite.as_u8(),
                len: 1,
            },
        );
    }

    #[test]
    fn identify_assembles_the_composite_identity_from_the_mock() {
        let mut executor = Executor::new_simulation(ExecutionTarget::Mock).unwrap();
        let mut transport = mock_transport();
        let identity = executor
            .identify(&mut transport, SessionState::Identifying)
            .unwrap();
        assert_eq!(&identity.variant.identifier, b"BTFL");
        assert_eq!(identity.version.major, 4);
        assert_eq!(identity.target_name, "SPEEDYBEEF405V4");
    }

    #[test]
    fn identification_runs_in_initial_post_reboot_and_recovery_contexts() {
        for state in [
            SessionState::Identifying,
            SessionState::Verifying,
            SessionState::Recovering,
        ] {
            let mut executor = Executor::new_simulation(ExecutionTarget::Mock).unwrap();
            let mut transport = mock_transport();
            let identity = executor.identify(&mut transport, state).unwrap();
            assert_eq!(&identity.variant.identifier, b"BTFL", "state {state:?}");
            assert_eq!(identity.version.major, 4, "state {state:?}");
            assert_eq!(identity.target_name, "SPEEDYBEEF405V4", "state {state:?}");
        }
    }

    #[test]
    fn incremental_and_native_identification_share_order_and_identity() {
        let mut executor = Executor::new_simulation(ExecutionTarget::Mock).unwrap();
        let mut transport = mock_transport();
        let native = executor
            .identify(&mut transport, SessionState::Identifying)
            .unwrap();

        let mut incremental = ReadonlyIdentification::new(SessionState::Identifying).unwrap();
        let mut fc = MockFc::new(snapshot());
        let mut commands = Vec::new();
        let stepped = loop {
            let request = incremental.next_request().unwrap();
            commands.push(request.command());
            let reply = decode_frame(&fc.respond(request.bytes()).unwrap()).unwrap();
            if let Some(identity) = incremental.accept_response(&reply).unwrap() {
                break identity;
            }
        };

        assert_eq!(
            commands,
            [
                CommandId::ApiVersion,
                CommandId::FcVariant,
                CommandId::FcVersion,
                CommandId::BoardInfo,
            ]
        );
        assert_eq!(stepped, native);
        assert!(incremental.is_complete());
        assert_eq!(
            incremental.next_request().unwrap_err(),
            ExecError::IdentificationAlreadyComplete
        );
    }

    #[test]
    fn incremental_identification_has_an_exhaustive_context_allowlist() {
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
            if state.permits_identification() {
                let mut identification = ReadonlyIdentification::new(state).unwrap();
                assert_eq!(
                    identification.next_request().unwrap().command(),
                    CommandId::ApiVersion,
                    "first request drifted for {state:?}",
                );
            } else {
                assert_eq!(
                    ReadonlyIdentification::new(state).unwrap_err(),
                    ExecError::IdentificationNotPermittedInState { state },
                );
            }
        }
    }

    #[test]
    fn incremental_identification_refuses_parallel_progress() {
        let mut identification = ReadonlyIdentification::new(SessionState::Identifying).unwrap();
        assert_eq!(
            identification
                .accept_response(
                    &decode_frame(
                        &encode_frame(Direction::Reply, CommandId::ApiVersion, &[0, 1, 46],)
                            .unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap_err(),
            ExecError::NoIdentificationRequestPending
        );
        identification.next_request().unwrap();
        assert_eq!(
            identification.next_request().unwrap_err(),
            ExecError::IdentificationRequestPending
        );
    }

    #[test]
    fn identity_stage_observation_survives_duplicate_and_out_of_order_failures() {
        let mut duplicate = ReadonlyIdentification::new(SessionState::Identifying).unwrap();
        assert_eq!(duplicate.stage(), IdentificationStage::ApiVersion);
        duplicate.next_request().unwrap();
        duplicate
            .accept_response(
                &decode_frame(
                    &encode_frame(Direction::Reply, CommandId::ApiVersion, &[0, 1, 46]).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(duplicate.stage(), IdentificationStage::FcVariant);
        duplicate.next_request().unwrap();
        assert_eq!(
            duplicate
                .accept_response(
                    &decode_frame(
                        &encode_frame(Direction::Reply, CommandId::ApiVersion, &[0, 1, 46])
                            .unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap_err(),
            ExecError::ReplyMisclassified(ReplyClass::Duplicate),
        );
        assert_eq!(duplicate.stage(), IdentificationStage::FcVariant);

        let mut out_of_order = ReadonlyIdentification::new(SessionState::Identifying).unwrap();
        out_of_order.next_request().unwrap();
        assert_eq!(
            out_of_order
                .accept_response(
                    &decode_frame(
                        &encode_frame(Direction::Reply, CommandId::FcVariant, b"BTFL").unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap_err(),
            ExecError::ReplyMisclassified(ReplyClass::Unsolicited),
        );
        assert_eq!(out_of_order.stage(), IdentificationStage::ApiVersion);
    }
}
