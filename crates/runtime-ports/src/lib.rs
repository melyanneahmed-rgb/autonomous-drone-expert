#![forbid(unsafe_code)]

//! # `ade-runtime-ports` — deterministic host-I/O effect boundary
//!
//! The safety core must run unchanged in a browser or a native shell, while Web Serial and
//! IndexedDB are asynchronous host facilities. This crate therefore performs **no I/O**.
//! It emits typed effects with request ids and accepts matching typed responses. A host may
//! execute those effects asynchronously, but completion order is validated before the core
//! can consume a result.
//!
//! Transport and storage may each have one request in flight. Writes remain structurally
//! bound to an [`ade_safety::WriteApproval`]; no new hardware-write authority is introduced.
//! It classifies the pinned command in each outbound frame before exposing bytes to a host,
//! but does not identify a flight controller or claim hardware support.

use ade_protocol_msp::{CommandId, Direction, decode_frame};
use ade_safety::{ExecutionTarget, RecoveryClass, WriteApproval, WriteCommandClass};
use core::fmt;

/// A monotonically allocated identifier for one host-I/O effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestId(u64);

impl RequestId {
    /// Reconstruct an id carried by a host callback. The coordinator still validates it
    /// against the exact pending request before accepting any response.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The numeric value used by a host adapter when returning a response.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A privacy-bounded key for local product storage.
///
/// It is intentionally not a path and cannot contain separators. The host supplies a case
/// key; it must never derive one from a serial number, USB uid, GPS coordinate or board
/// signature.
#[derive(Clone, PartialEq, Eq)]
pub struct StorageKey(String);

impl fmt::Debug for StorageKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StorageKey(<redacted>)")
    }
}

impl StorageKey {
    /// Validate a storage key of 1..=64 lowercase ASCII letters, digits, `_` or `-`.
    ///
    /// # Errors
    /// Returns [`BoundaryError::InvalidStorageKey`] for an empty, oversized or invalid key.
    pub fn new(value: impl Into<String>) -> Result<Self, BoundaryError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 64
            || !value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte)
            })
        {
            return Err(BoundaryError::InvalidStorageKey);
        }
        Ok(Self(value))
    }

    /// The validated adapter key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A host-reported storage revision used for compare-and-swap commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageRevision(u64);

impl StorageRevision {
    /// Construct a revision returned by a trusted storage adapter.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PacketAuthority {
    ReadOnly,
    Approved(WriteApproval),
}

/// An outbound packet whose command class and write authority are fixed at construction.
///
/// Protocol-specific audit still validates the actual frame. This wrapper prevents the host
/// adapter from being handed a write-class packet unless simulation write evidence already
/// exists.
#[derive(Clone, PartialEq, Eq)]
pub struct OutboundPacket {
    bytes: Vec<u8>,
    class: WriteCommandClass,
    authority: PacketAuthority,
}

impl fmt::Debug for OutboundPacket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OutboundPacket")
            .field("byte_len", &self.bytes.len())
            .field("class", &self.class)
            .field("approved_target", &self.approved_target())
            .field("approved_recovery", &self.approved_recovery())
            .finish()
    }
}

impl OutboundPacket {
    /// Construct a non-empty read/identify packet with no write authority.
    ///
    /// # Errors
    /// Returns [`BoundaryError::EmptyPacket`] for an empty packet.
    pub fn read_only(bytes: Vec<u8>) -> Result<Self, BoundaryError> {
        if bytes.is_empty() {
            return Err(BoundaryError::EmptyPacket);
        }
        let actual = classify_packet(&bytes)?;
        if actual != WriteCommandClass::NoWrite {
            return Err(BoundaryError::PacketRequiresApproval { actual });
        }
        Ok(Self {
            bytes,
            class: WriteCommandClass::NoWrite,
            authority: PacketAuthority::ReadOnly,
        })
    }

    /// Construct a non-empty write/reboot packet from existing simulation approval evidence.
    ///
    /// There is no constructor that accepts a target or command class directly. Hardware
    /// approval remains impossible because [`WriteApproval`] cannot be created for hardware.
    ///
    /// # Errors
    /// Returns [`BoundaryError::EmptyPacket`] for an empty packet.
    pub fn approved(bytes: Vec<u8>, approval: WriteApproval) -> Result<Self, BoundaryError> {
        if bytes.is_empty() {
            return Err(BoundaryError::EmptyPacket);
        }
        let actual = classify_packet(&bytes)?;
        if actual != approval.class() {
            return Err(BoundaryError::PacketClassMismatch {
                actual,
                approved: approval.class(),
            });
        }
        Ok(Self {
            bytes,
            class: approval.class(),
            authority: PacketAuthority::Approved(approval),
        })
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub const fn class(&self) -> WriteCommandClass {
        self.class
    }

    /// The simulation target proven by the write approval, or `None` for a read.
    #[must_use]
    pub const fn approved_target(&self) -> Option<ExecutionTarget> {
        match &self.authority {
            PacketAuthority::ReadOnly => None,
            PacketAuthority::Approved(approval) => Some(approval.target()),
        }
    }

    /// The declared recovery posture proven by the write approval, or `None` for a read.
    #[must_use]
    pub const fn approved_recovery(&self) -> Option<RecoveryClass> {
        match &self.authority {
            PacketAuthority::ReadOnly => None,
            PacketAuthority::Approved(approval) => Some(approval.recovery()),
        }
    }

    /// The original typed approval evidence, or `None` for a proven read.
    #[must_use]
    pub const fn approval(&self) -> Option<&WriteApproval> {
        match &self.authority {
            PacketAuthority::ReadOnly => None,
            PacketAuthority::Approved(approval) => Some(approval),
        }
    }
}

fn classify_packet(bytes: &[u8]) -> Result<WriteCommandClass, BoundaryError> {
    let frame = decode_frame(bytes).map_err(|_| BoundaryError::InvalidTransportPacket)?;
    if frame.direction != Direction::Request {
        return Err(BoundaryError::InvalidTransportPacket);
    }
    let command = frame
        .known_command()
        .ok_or(BoundaryError::InvalidTransportPacket)?;
    Ok(match command {
        CommandId::SetBeeperConfig => WriteCommandClass::TransientConfig,
        CommandId::EepromWrite => WriteCommandClass::PersistentConfig,
        CommandId::Reboot => WriteCommandClass::Reboot,
        CommandId::ApiVersion
        | CommandId::FcVariant
        | CommandId::FcVersion
        | CommandId::BoardInfo
        | CommandId::BeeperConfig => WriteCommandClass::NoWrite,
    })
}

/// An effect for the host-owned transport adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportEffect {
    /// Open only the port the user selected. This carries no write authority.
    OpenSelectedReadOnlyPort,
    /// Exchange one executor-produced packet.
    Exchange(OutboundPacket),
    /// Close the currently selected port.
    Close,
}

/// An effect for local, host-owned storage.
#[derive(Clone, PartialEq, Eq)]
pub enum StorageEffect {
    /// Load the latest committed bytes and revision for a case key.
    Load { key: StorageKey },
    /// Atomically commit the complete new value if `expected_revision` still matches.
    ///
    /// IndexedDB implements this in one transaction; a native adapter may use an atomic
    /// replacement. A conflict is reported, never overwritten silently.
    CompareAndSwap {
        key: StorageKey,
        expected_revision: Option<StorageRevision>,
        bytes: Vec<u8>,
    },
}

impl fmt::Debug for StorageEffect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load { key } => formatter.debug_struct("Load").field("key", key).finish(),
            Self::CompareAndSwap {
                key,
                expected_revision,
                bytes,
            } => formatter
                .debug_struct("CompareAndSwap")
                .field("key", key)
                .field("expected_revision", expected_revision)
                .field("byte_len", &bytes.len())
                .finish(),
        }
    }
}

/// A typed request emitted by the deterministic core for a host adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IoEffect {
    Transport {
        request_id: RequestId,
        effect: TransportEffect,
    },
    Storage {
        request_id: RequestId,
        effect: StorageEffect,
    },
}

impl IoEffect {
    #[must_use]
    pub const fn request_id(&self) -> RequestId {
        match self {
            Self::Transport { request_id, .. } | Self::Storage { request_id, .. } => *request_id,
        }
    }
}

/// Stable transport failures returned by browser or native adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportFailure {
    PortBusy,
    PermissionDenied,
    MissingDriver,
    Disconnected,
    Timeout,
    Cancelled,
    Unknown,
}

/// Stable local-storage failures returned by browser or native adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageFailure {
    Conflict,
    QuotaExceeded,
    Unavailable,
    Corrupt,
    Cancelled,
    Unknown,
}

/// The result of one transport effect.
#[derive(Clone, PartialEq, Eq)]
pub enum TransportResult {
    Open(Result<(), TransportFailure>),
    Exchange(Result<Vec<u8>, TransportFailure>),
    Close(Result<(), TransportFailure>),
}

impl fmt::Debug for TransportResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(result) => formatter.debug_tuple("Open").field(result).finish(),
            Self::Exchange(Ok(bytes)) => formatter
                .debug_struct("Exchange")
                .field("byte_len", &bytes.len())
                .finish(),
            Self::Exchange(Err(error)) => formatter
                .debug_struct("Exchange")
                .field("error", error)
                .finish(),
            Self::Close(result) => formatter.debug_tuple("Close").field(result).finish(),
        }
    }
}

/// A value loaded atomically with its current storage revision.
#[derive(Clone, PartialEq, Eq)]
pub struct StoredValue {
    pub revision: StorageRevision,
    pub bytes: Vec<u8>,
}

impl fmt::Debug for StoredValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredValue")
            .field("revision", &self.revision)
            .field("byte_len", &self.bytes.len())
            .finish()
    }
}

/// The result of one storage effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageResult {
    Load(Result<Option<StoredValue>, StorageFailure>),
    Commit(Result<StorageRevision, StorageFailure>),
}

/// A typed host response returned to the deterministic core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IoResponse {
    Transport {
        request_id: RequestId,
        result: TransportResult,
    },
    Storage {
        request_id: RequestId,
        result: StorageResult,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponseKind {
    TransportOpen,
    TransportExchange,
    TransportClose,
    StorageLoad,
    StorageCommit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pending {
    request_id: RequestId,
    expected: ResponseKind,
}

/// Why an effect could not be emitted or a host response was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryError {
    InvalidStorageKey,
    EmptyPacket,
    InvalidTransportPacket,
    PacketRequiresApproval {
        actual: WriteCommandClass,
    },
    PacketClassMismatch {
        actual: WriteCommandClass,
        approved: WriteCommandClass,
    },
    EmptyStorageValue,
    RequestIdExhausted,
    TransportRequestAlreadyPending,
    StorageRequestAlreadyPending,
    NoTransportRequestPending,
    NoStorageRequestPending,
    RequestIdMismatch {
        expected: RequestId,
        received: RequestId,
    },
    ResponseKindMismatch,
}

/// Coordinates effect ids and refuses stale, duplicated or wrong-kind responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoCoordinator {
    next_request_id: u64,
    pending_transport: Option<Pending>,
    pending_storage: Option<Pending>,
}

impl Default for IoCoordinator {
    fn default() -> Self {
        Self {
            next_request_id: 1,
            pending_transport: None,
            pending_storage: None,
        }
    }
}

impl IoCoordinator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn allocate(&mut self) -> Result<RequestId, BoundaryError> {
        let request_id = RequestId(self.next_request_id);
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or(BoundaryError::RequestIdExhausted)?;
        Ok(request_id)
    }

    /// Emit a transport effect. Only one transport operation may be in flight.
    ///
    /// # Errors
    /// Returns [`BoundaryError::TransportRequestAlreadyPending`] until the pending response
    /// is accepted, or [`BoundaryError::RequestIdExhausted`] on counter exhaustion.
    pub fn begin_transport(&mut self, effect: TransportEffect) -> Result<IoEffect, BoundaryError> {
        if self.pending_transport.is_some() {
            return Err(BoundaryError::TransportRequestAlreadyPending);
        }
        let expected = match &effect {
            TransportEffect::OpenSelectedReadOnlyPort => ResponseKind::TransportOpen,
            TransportEffect::Exchange(_) => ResponseKind::TransportExchange,
            TransportEffect::Close => ResponseKind::TransportClose,
        };
        let request_id = self.allocate()?;
        self.pending_transport = Some(Pending {
            request_id,
            expected,
        });
        Ok(IoEffect::Transport { request_id, effect })
    }

    /// Emit a storage effect. Transport and storage may progress independently, but only one
    /// storage operation may be in flight.
    ///
    /// # Errors
    /// Returns a typed pending/id/value error.
    pub fn begin_storage(&mut self, effect: StorageEffect) -> Result<IoEffect, BoundaryError> {
        if self.pending_storage.is_some() {
            return Err(BoundaryError::StorageRequestAlreadyPending);
        }
        if matches!(
            &effect,
            StorageEffect::CompareAndSwap { bytes, .. } if bytes.is_empty()
        ) {
            return Err(BoundaryError::EmptyStorageValue);
        }
        let expected = match &effect {
            StorageEffect::Load { .. } => ResponseKind::StorageLoad,
            StorageEffect::CompareAndSwap { .. } => ResponseKind::StorageCommit,
        };
        let request_id = self.allocate()?;
        self.pending_storage = Some(Pending {
            request_id,
            expected,
        });
        Ok(IoEffect::Storage { request_id, effect })
    }

    /// Validate and consume one host response. A mismatched response never clears pending
    /// state, so a stale or malicious completion cannot advance the core.
    ///
    /// # Errors
    /// Returns a typed lane, id or response-kind mismatch.
    pub fn accept(&mut self, response: IoResponse) -> Result<IoResponse, BoundaryError> {
        let (pending, received_id, received_kind) = match &response {
            IoResponse::Transport { request_id, result } => {
                let pending = self
                    .pending_transport
                    .ok_or(BoundaryError::NoTransportRequestPending)?;
                (pending, *request_id, transport_result_kind(result))
            }
            IoResponse::Storage { request_id, result } => {
                let pending = self
                    .pending_storage
                    .ok_or(BoundaryError::NoStorageRequestPending)?;
                (pending, *request_id, storage_result_kind(result))
            }
        };
        if pending.request_id != received_id {
            return Err(BoundaryError::RequestIdMismatch {
                expected: pending.request_id,
                received: received_id,
            });
        }
        if pending.expected != received_kind {
            return Err(BoundaryError::ResponseKindMismatch);
        }
        match &response {
            IoResponse::Transport { .. } => self.pending_transport = None,
            IoResponse::Storage { .. } => self.pending_storage = None,
        }
        Ok(response)
    }

    /// Clear the exact pending transport request after the host reports a transport-layer
    /// failure outside [`IoResponse`]. A stale id cannot clear a newer request.
    ///
    /// # Errors
    /// Returns the same lane/id errors as [`Self::accept`] without changing pending state.
    pub fn cancel_transport(&mut self, request_id: RequestId) -> Result<(), BoundaryError> {
        let pending = self
            .pending_transport
            .ok_or(BoundaryError::NoTransportRequestPending)?;
        if pending.request_id != request_id {
            return Err(BoundaryError::RequestIdMismatch {
                expected: pending.request_id,
                received: request_id,
            });
        }
        self.pending_transport = None;
        Ok(())
    }

    #[must_use]
    pub const fn has_pending_transport(&self) -> bool {
        self.pending_transport.is_some()
    }

    #[must_use]
    pub const fn has_pending_storage(&self) -> bool {
        self.pending_storage.is_some()
    }
}

fn transport_result_kind(result: &TransportResult) -> ResponseKind {
    match result {
        TransportResult::Open(_) => ResponseKind::TransportOpen,
        TransportResult::Exchange(_) => ResponseKind::TransportExchange,
        TransportResult::Close(_) => ResponseKind::TransportClose,
    }
}

fn storage_result_kind(result: &StorageResult) -> ResponseKind {
    match result {
        StorageResult::Load(_) => ResponseKind::StorageLoad,
        StorageResult::Commit(_) => ResponseKind::StorageCommit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ade_protocol_msp::{CommandId, Direction, encode_frame};
    use ade_safety::authorize_write;

    fn key() -> StorageKey {
        StorageKey::new("case-0001").unwrap()
    }

    fn request(command: CommandId, payload: &[u8]) -> Vec<u8> {
        encode_frame(Direction::Request, command, payload).unwrap()
    }

    #[test]
    fn storage_keys_are_bounded_and_are_not_paths() {
        assert_eq!(StorageKey::new(""), Err(BoundaryError::InvalidStorageKey));
        assert_eq!(
            StorageKey::new("case/serial"),
            Err(BoundaryError::InvalidStorageKey)
        );
        assert_eq!(StorageKey::new("A"), Err(BoundaryError::InvalidStorageKey));
        assert_eq!(key().as_str(), "case-0001");
    }

    #[test]
    fn read_packets_carry_no_write_authority() {
        let packet = OutboundPacket::read_only(request(CommandId::ApiVersion, &[])).unwrap();
        assert_eq!(packet.class(), WriteCommandClass::NoWrite);
        assert_eq!(packet.approved_target(), None);
        assert_eq!(packet.approved_recovery(), None);
        assert_eq!(
            OutboundPacket::read_only(Vec::new()),
            Err(BoundaryError::EmptyPacket)
        );
    }

    #[test]
    fn write_packets_preserve_existing_simulation_approval() {
        let approval = authorize_write(
            ExecutionTarget::Replay,
            WriteCommandClass::TransientConfig,
            RecoveryClass::RestoreFromBackupSupported,
        )
        .unwrap();
        let packet =
            OutboundPacket::approved(request(CommandId::SetBeeperConfig, &[1, 2, 3, 4]), approval)
                .unwrap();
        assert_eq!(packet.class(), WriteCommandClass::TransientConfig);
        assert_eq!(packet.approved_target(), Some(ExecutionTarget::Replay));
        assert_eq!(
            packet.approved_recovery(),
            Some(RecoveryClass::RestoreFromBackupSupported)
        );
    }

    #[test]
    fn packet_construction_refuses_relabelled_commands_and_borrowed_approval() {
        let set = request(CommandId::SetBeeperConfig, &[1, 2, 3, 4]);
        assert_eq!(
            OutboundPacket::read_only(set),
            Err(BoundaryError::PacketRequiresApproval {
                actual: WriteCommandClass::TransientConfig,
            })
        );

        let transient = authorize_write(
            ExecutionTarget::Mock,
            WriteCommandClass::TransientConfig,
            RecoveryClass::TransientWritePendingReconcileOnResume,
        )
        .unwrap();
        assert_eq!(
            OutboundPacket::approved(request(CommandId::ApiVersion, &[]), transient.clone()),
            Err(BoundaryError::PacketClassMismatch {
                actual: WriteCommandClass::NoWrite,
                approved: WriteCommandClass::TransientConfig,
            })
        );
        assert_eq!(
            OutboundPacket::approved(request(CommandId::EepromWrite, &[]), transient),
            Err(BoundaryError::PacketClassMismatch {
                actual: WriteCommandClass::PersistentConfig,
                approved: WriteCommandClass::TransientConfig,
            })
        );
    }

    #[test]
    fn lanes_progress_independently_but_each_lane_is_single_flight() {
        let mut coordinator = IoCoordinator::new();
        let transport = coordinator
            .begin_transport(TransportEffect::OpenSelectedReadOnlyPort)
            .unwrap();
        let storage = coordinator
            .begin_storage(StorageEffect::Load { key: key() })
            .unwrap();
        assert_ne!(transport.request_id(), storage.request_id());
        assert!(coordinator.has_pending_transport());
        assert!(coordinator.has_pending_storage());
        assert_eq!(
            coordinator.begin_transport(TransportEffect::Close),
            Err(BoundaryError::TransportRequestAlreadyPending)
        );
        assert_eq!(
            coordinator.begin_storage(StorageEffect::Load { key: key() }),
            Err(BoundaryError::StorageRequestAlreadyPending)
        );
    }

    #[test]
    fn stale_or_wrong_kind_responses_never_clear_pending_state() {
        let mut coordinator = IoCoordinator::new();
        let effect = coordinator
            .begin_transport(TransportEffect::OpenSelectedReadOnlyPort)
            .unwrap();
        let expected = effect.request_id();
        let stale = RequestId(expected.get() + 1);
        assert_eq!(
            coordinator.accept(IoResponse::Transport {
                request_id: stale,
                result: TransportResult::Open(Ok(())),
            }),
            Err(BoundaryError::RequestIdMismatch {
                expected,
                received: stale,
            })
        );
        assert!(coordinator.has_pending_transport());
        assert_eq!(
            coordinator.accept(IoResponse::Transport {
                request_id: expected,
                result: TransportResult::Close(Ok(())),
            }),
            Err(BoundaryError::ResponseKindMismatch)
        );
        assert!(coordinator.has_pending_transport());
        coordinator
            .accept(IoResponse::Transport {
                request_id: expected,
                result: TransportResult::Open(Ok(())),
            })
            .unwrap();
        assert!(!coordinator.has_pending_transport());
    }

    #[test]
    fn only_the_exact_pending_transport_can_be_cancelled() {
        let mut coordinator = IoCoordinator::new();
        let effect = coordinator
            .begin_transport(TransportEffect::OpenSelectedReadOnlyPort)
            .unwrap();
        let expected = effect.request_id();
        let stale = RequestId(expected.get() + 1);
        assert_eq!(
            coordinator.cancel_transport(stale),
            Err(BoundaryError::RequestIdMismatch {
                expected,
                received: stale,
            })
        );
        assert!(coordinator.has_pending_transport());
        coordinator.cancel_transport(expected).unwrap();
        assert!(!coordinator.has_pending_transport());
    }

    #[test]
    fn storage_commits_are_revision_checked_and_non_empty() {
        let mut coordinator = IoCoordinator::new();
        assert_eq!(
            coordinator.begin_storage(StorageEffect::CompareAndSwap {
                key: key(),
                expected_revision: None,
                bytes: Vec::new(),
            }),
            Err(BoundaryError::EmptyStorageValue)
        );
        let effect = coordinator
            .begin_storage(StorageEffect::CompareAndSwap {
                key: key(),
                expected_revision: Some(StorageRevision::new(4)),
                bytes: vec![1, 2, 3],
            })
            .unwrap();
        let id = effect.request_id();
        coordinator
            .accept(IoResponse::Storage {
                request_id: id,
                result: StorageResult::Commit(Ok(StorageRevision::new(5))),
            })
            .unwrap();
        assert!(!coordinator.has_pending_storage());
    }

    #[test]
    fn debug_output_redacts_keys_and_raw_bytes() {
        let sensitive_text = "serial-uid-gps-coordinate";
        let sensitive_bytes = sensitive_text.as_bytes().to_vec();
        let storage_key = StorageKey::new("case-sensitive-key").unwrap();

        let key_debug = format!("{storage_key:?}");
        assert!(!key_debug.contains("case-sensitive-key"));

        let packet =
            OutboundPacket::read_only(request(CommandId::ApiVersion, &sensitive_bytes)).unwrap();
        let packet_debug = format!("{packet:?}");
        assert!(!packet_debug.contains(sensitive_text));
        assert!(packet_debug.contains("byte_len"));

        let storage_effect = StorageEffect::CompareAndSwap {
            key: storage_key,
            expected_revision: Some(StorageRevision::new(2)),
            bytes: sensitive_bytes.clone(),
        };
        let storage_effect_debug = format!("{storage_effect:?}");
        assert!(!storage_effect_debug.contains(sensitive_text));
        assert!(!storage_effect_debug.contains("case-sensitive-key"));

        let transport_response = IoResponse::Transport {
            request_id: RequestId(7),
            result: TransportResult::Exchange(Ok(sensitive_bytes.clone())),
        };
        assert!(!format!("{transport_response:?}").contains(sensitive_text));

        let storage_response = IoResponse::Storage {
            request_id: RequestId(8),
            result: StorageResult::Load(Ok(Some(StoredValue {
                revision: StorageRevision::new(3),
                bytes: sensitive_bytes,
            }))),
        };
        assert!(!format!("{storage_response:?}").contains(sensitive_text));
    }
}
