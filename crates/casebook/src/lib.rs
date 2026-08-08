#![forbid(unsafe_code)]

//! # `ade-casebook` — local case journal and resume reconciliation (M1)
//!
//! A local-only, append-only [`Journal`] of lifecycle events, from which the orchestrator
//! can be deterministically reconstructed after a process restart. The [`CaseRecord`] schema
//! is versioned and, by construction, carries **no** hardware-derived or tracking identifier:
//! the case id is supplied by the host and must not be derived from the device.

use ade_facts::DeviceIdentity;
use ade_runtime_ports::{
    BoundaryError, IoCoordinator, IoEffect, IoResponse, StorageEffect, StorageFailure, StorageKey,
    StorageResult, StorageRevision, StoredValue,
};
use ade_safety::{ExecutionTarget, RecoveryClass, WriteCommandClass};
use std::fmt;
use std::io::ErrorKind;

const JOURNAL_MAGIC: &[u8; 4] = b"ADEJ";
const JOURNAL_VERSION: u16 = 1;
const HEADER_LEN: usize = 8;
const RECORD_OVERHEAD: usize = 8;
const MAX_RECORD_PAYLOAD_BYTES: usize = 64;

/// Default upper bound for a journal, including its header and record framing.
pub const DEFAULT_MAX_JOURNAL_BYTES: usize = 64 * 1024;

/// Fixed byte length of the ADEJ header.
pub const JOURNAL_HEADER_LEN: usize = HEADER_LEN;

/// The current schema version of a [`CaseRecord`].
pub const CASE_SCHEMA_VERSION: u32 = 1;

/// One recorded lifecycle event. Ordering in a [`Journal`] is authoritative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalEvent {
    /// The case started against a simulation target.
    Started { execution_target: ExecutionTarget },
    /// Identity was read.
    IdentityRead,
    /// The configuration snapshot was read.
    SnapshotRead,
    /// A backup was written.
    BackedUp,
    /// A transient (RAM-only) write was applied but not yet saved.
    TransientWriteApplied { field: &'static str, mask: u32 },
    /// The configuration was re-read before saving.
    ReReadBeforeSave,
    /// Durable write-ahead evidence recorded and synced before a write or reboot is sent.
    WriteAhead {
        /// The outbound command class.
        class: WriteCommandClass,
        /// The recovery posture bound to that command.
        recovery: RecoveryClass,
    },
    /// The configuration was committed to persistent storage.
    Saved,
    /// The device was rebooted.
    Rebooted,
    /// The connection was re-established.
    Reconnected,
    /// The intended change (and only it) was verified.
    Verified,
    /// A recovery was started.
    RecoveryStarted,
    /// The previous value was restored and verified.
    Restored,
    /// The state could not be proven.
    StateUnknown,
}

/// Why a durable journal could not be opened, validated or appended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalError {
    /// An operating-system file error, reduced to its stable kind.
    Io(ErrorKind),
    /// The requested byte bound cannot hold even the file header.
    LimitTooSmall,
    /// A create-new operation refused to overwrite an existing path.
    AlreadyExists,
    /// The file does not start with the `ADEJ` magic.
    InvalidMagic,
    /// The on-disk format version is unsupported.
    UnsupportedVersion(u16),
    /// Header reserved bytes were non-zero.
    InvalidHeader,
    /// A complete record exceeded the explicit record-size bound.
    RecordTooLarge(u32),
    /// A complete record's checksum did not match its payload.
    ChecksumMismatch { offset: usize },
    /// A complete payload could not be decoded into a journal event.
    InvalidRecord { offset: usize },
    /// Appending the record would exceed the journal byte bound.
    Full { limit: usize },
    /// A previous durable append failed after the file position became unprovable.
    Poisoned,
    /// A prepared append was accepted against a different logical journal position.
    StalePreparedAppend { expected: usize, actual: usize },
    /// The durable backend length did not match the logical journal boundary.
    BackendPositionMismatch { expected: usize, actual: usize },
}

impl fmt::Display for JournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for JournalError {}

impl From<std::io::Error> for JournalError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.kind())
    }
}

/// A validated record that has not yet been durably accepted.
///
/// Preparing never mutates the journal. The logical event becomes visible only after a
/// backend has durably accepted `record_bytes` and [`Journal::accept_prepared`] succeeds.
#[derive(Clone, PartialEq, Eq)]
pub struct PreparedJournalAppend {
    event: JournalEvent,
    record_bytes: Vec<u8>,
    expected_len: usize,
    next_len: usize,
}

impl fmt::Debug for PreparedJournalAppend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedJournalAppend")
            .field("record_byte_len", &self.record_bytes.len())
            .field("expected_len", &self.expected_len)
            .field("next_len", &self.next_len)
            .finish()
    }
}

impl PreparedJournalAppend {
    #[must_use]
    pub fn record_bytes(&self) -> &[u8] {
        &self.record_bytes
    }

    #[must_use]
    pub const fn expected_len(&self) -> usize {
        self.expected_len
    }

    #[must_use]
    pub const fn next_len(&self) -> usize {
        self.next_len
    }
}

/// Durable append backend. Implementations must return `Ok(())` only after the entire
/// record has been written, flushed, and synchronised according to their durability model.
pub trait JournalBackend: fmt::Debug {
    /// Durably accept one previously validated append.
    ///
    /// # Errors
    /// Returns a stable [`JournalError`] without claiming success after an uncertain write.
    fn append_durable(&mut self, prepared: &PreparedJournalAppend) -> Result<(), JournalError>;
}

/// An append-only journal of lifecycle events (local only, never uploaded).
///
/// [`Journal::decode`] validates existing ADEJ bytes and identifies only an incomplete final
/// record as repairable. [`Journal::try_append`] prepares a complete record and, when a
/// backend is attached, requires durable backend success before making the event visible.
pub struct Journal {
    events: Vec<JournalEvent>,
    backend: Option<Box<dyn JournalBackend>>,
    max_bytes: usize,
    encoded_len: usize,
    last_error: Option<JournalError>,
    poisoned: bool,
}

impl fmt::Debug for Journal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Journal")
            .field("event_count", &self.events.len())
            .field("backend_attached", &self.backend.is_some())
            .field("max_bytes", &self.max_bytes)
            .field("encoded_len", &self.encoded_len)
            .field("last_error", &self.last_error)
            .field("poisoned", &self.poisoned)
            .finish()
    }
}

impl Default for Journal {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            backend: None,
            max_bytes: DEFAULT_MAX_JOURNAL_BYTES,
            encoded_len: HEADER_LEN,
            last_error: None,
            poisoned: false,
        }
    }
}

impl Clone for Journal {
    fn clone(&self) -> Self {
        // A clone is intentionally detached from the backend. Two appenders to one case
        // journal would violate the single-writer ordering contract.
        Self {
            events: self.events.clone(),
            backend: None,
            max_bytes: self.max_bytes,
            encoded_len: self.encoded_len,
            last_error: self.last_error.clone(),
            poisoned: self.poisoned,
        }
    }
}

impl PartialEq for Journal {
    fn eq(&self, other: &Self) -> bool {
        self.events == other.events
    }
}

impl Eq for Journal {}

fn encode_target(target: ExecutionTarget) -> u8 {
    match target {
        ExecutionTarget::Mock => 0,
        ExecutionTarget::Replay => 1,
        ExecutionTarget::Hardware => 2,
    }
}

fn decode_target(value: u8) -> Option<ExecutionTarget> {
    match value {
        0 => Some(ExecutionTarget::Mock),
        1 => Some(ExecutionTarget::Replay),
        2 => Some(ExecutionTarget::Hardware),
        _ => None,
    }
}

fn encode_write_class(class: WriteCommandClass) -> u8 {
    match class {
        WriteCommandClass::NoWrite => 0,
        WriteCommandClass::TransientConfig => 1,
        WriteCommandClass::PersistentConfig => 2,
        WriteCommandClass::Reboot => 3,
    }
}

fn decode_write_class(value: u8) -> Option<WriteCommandClass> {
    match value {
        0 => Some(WriteCommandClass::NoWrite),
        1 => Some(WriteCommandClass::TransientConfig),
        2 => Some(WriteCommandClass::PersistentConfig),
        3 => Some(WriteCommandClass::Reboot),
        _ => None,
    }
}

fn encode_recovery_class(class: RecoveryClass) -> u8 {
    match class {
        RecoveryClass::NotApplicableNoWrite => 0,
        RecoveryClass::TransientWritePendingReconcileOnResume => 1,
        RecoveryClass::AutomaticRollbackSupported => 2,
        RecoveryClass::RestoreFromBackupSupported => 3,
        RecoveryClass::ManualRecoveryRequired => 4,
        RecoveryClass::StateUnknownRecoveryRequired => 5,
    }
}

fn decode_recovery_class(value: u8) -> Option<RecoveryClass> {
    match value {
        0 => Some(RecoveryClass::NotApplicableNoWrite),
        1 => Some(RecoveryClass::TransientWritePendingReconcileOnResume),
        2 => Some(RecoveryClass::AutomaticRollbackSupported),
        3 => Some(RecoveryClass::RestoreFromBackupSupported),
        4 => Some(RecoveryClass::ManualRecoveryRequired),
        5 => Some(RecoveryClass::StateUnknownRecoveryRequired),
        _ => None,
    }
}

fn encode_event(event: &JournalEvent) -> Vec<u8> {
    let mut payload = Vec::with_capacity(8);
    match event {
        JournalEvent::Started { execution_target } => {
            payload.extend([1, encode_target(*execution_target)]);
        }
        JournalEvent::IdentityRead => payload.push(2),
        JournalEvent::SnapshotRead => payload.push(3),
        JournalEvent::BackedUp => payload.push(4),
        JournalEvent::TransientWriteApplied { field, mask } => {
            payload.push(5);
            // M1 has exactly one writable field. Any other static label remains representable
            // in memory but cannot be persisted as trusted resume evidence.
            payload.push(u8::from(*field == "beeper_off_flags"));
            payload.extend(mask.to_le_bytes());
        }
        JournalEvent::ReReadBeforeSave => payload.push(6),
        JournalEvent::Saved => payload.push(7),
        JournalEvent::Rebooted => payload.push(8),
        JournalEvent::Reconnected => payload.push(9),
        JournalEvent::Verified => payload.push(10),
        JournalEvent::RecoveryStarted => payload.push(11),
        JournalEvent::Restored => payload.push(12),
        JournalEvent::StateUnknown => payload.push(13),
        JournalEvent::WriteAhead { class, recovery } => {
            payload.extend([
                14,
                encode_write_class(*class),
                encode_recovery_class(*recovery),
            ]);
        }
    }
    payload
}

fn decode_event(payload: &[u8]) -> Option<JournalEvent> {
    match payload {
        [1, target] => Some(JournalEvent::Started {
            execution_target: decode_target(*target)?,
        }),
        [2] => Some(JournalEvent::IdentityRead),
        [3] => Some(JournalEvent::SnapshotRead),
        [4] => Some(JournalEvent::BackedUp),
        [5, 1, a, b, c, d] => Some(JournalEvent::TransientWriteApplied {
            field: "beeper_off_flags",
            mask: u32::from_le_bytes([*a, *b, *c, *d]),
        }),
        [6] => Some(JournalEvent::ReReadBeforeSave),
        [7] => Some(JournalEvent::Saved),
        [8] => Some(JournalEvent::Rebooted),
        [9] => Some(JournalEvent::Reconnected),
        [10] => Some(JournalEvent::Verified),
        [11] => Some(JournalEvent::RecoveryStarted),
        [12] => Some(JournalEvent::Restored),
        [13] => Some(JournalEvent::StateUnknown),
        [14, class, recovery] => Some(JournalEvent::WriteAhead {
            class: decode_write_class(*class)?,
            recovery: decode_recovery_class(*recovery)?,
        }),
        _ => None,
    }
}

fn checksum(payload: &[u8]) -> u32 {
    // FNV-1a is used only as a deterministic torn/corrupt-record detector, not as a
    // cryptographic authenticity claim.
    payload.iter().fold(0x811c_9dc5, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193)
    })
}

fn header() -> [u8; HEADER_LEN] {
    let mut bytes = [0_u8; HEADER_LEN];
    bytes[..4].copy_from_slice(JOURNAL_MAGIC);
    bytes[4..6].copy_from_slice(&JOURNAL_VERSION.to_le_bytes());
    bytes
}

/// The initial bytes a durable backend must synchronise before attaching to an empty journal.
#[must_use]
pub fn empty_journal_bytes() -> [u8; HEADER_LEN] {
    header()
}

/// A validated ADEJ decode and its optional torn-final-tail repair boundary.
#[derive(Debug)]
pub struct DecodedJournal {
    journal: Journal,
    repair_to: Option<usize>,
    input_len: usize,
}

impl DecodedJournal {
    /// Proven byte boundary to truncate to when the input ended in an incomplete final record.
    #[must_use]
    pub const fn repair_to(&self) -> Option<usize> {
        self.repair_to
    }

    /// Original input length before any host-side repair.
    #[must_use]
    pub const fn input_len(&self) -> usize {
        self.input_len
    }

    /// Consume the decode into a backend-free logical journal.
    #[must_use]
    pub fn into_journal(self) -> Journal {
        self.journal
    }
}

impl Journal {
    /// A new, empty journal.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a bounded in-memory journal, useful for deterministic tests.
    ///
    /// # Errors
    /// [`JournalError::LimitTooSmall`] if `max_bytes` cannot hold the header.
    pub fn with_limit(max_bytes: usize) -> Result<Self, JournalError> {
        if max_bytes < HEADER_LEN {
            return Err(JournalError::LimitTooSmall);
        }
        Ok(Self {
            max_bytes,
            ..Self::default()
        })
    }

    /// Decode bounded ADEJ bytes without touching a filesystem or mutating storage.
    ///
    /// A short final record is reported through [`DecodedJournal::repair_to`]. Complete
    /// checksum, payload, magic, version, or reserved-byte corruption is refused.
    ///
    /// # Errors
    /// Returns a typed format or bound failure.
    pub fn decode(bytes: &[u8], max_bytes: usize) -> Result<DecodedJournal, JournalError> {
        if max_bytes < HEADER_LEN {
            return Err(JournalError::LimitTooSmall);
        }
        if bytes.len() > max_bytes {
            return Err(JournalError::Full { limit: max_bytes });
        }
        if bytes.len() < HEADER_LEN || &bytes[..4] != JOURNAL_MAGIC {
            return Err(JournalError::InvalidMagic);
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != JOURNAL_VERSION {
            return Err(JournalError::UnsupportedVersion(version));
        }
        if bytes[6..8] != [0, 0] {
            return Err(JournalError::InvalidHeader);
        }

        let mut events = Vec::new();
        let mut offset = HEADER_LEN;
        let mut proven_len = HEADER_LEN;
        while offset < bytes.len() {
            let remaining = bytes.len() - offset;
            if remaining < 4 {
                break;
            }
            let payload_len = u32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]);
            if payload_len as usize > MAX_RECORD_PAYLOAD_BYTES {
                return Err(JournalError::RecordTooLarge(payload_len));
            }
            let record_len = RECORD_OVERHEAD + payload_len as usize;
            if remaining < record_len {
                break;
            }
            let payload_start = offset + 4;
            let payload_end = payload_start + payload_len as usize;
            let expected = u32::from_le_bytes([
                bytes[payload_end],
                bytes[payload_end + 1],
                bytes[payload_end + 2],
                bytes[payload_end + 3],
            ]);
            let payload = &bytes[payload_start..payload_end];
            if checksum(payload) != expected {
                return Err(JournalError::ChecksumMismatch { offset });
            }
            let Some(event) = decode_event(payload) else {
                return Err(JournalError::InvalidRecord { offset });
            };
            events.push(event);
            offset += record_len;
            proven_len = offset;
        }

        Ok(DecodedJournal {
            journal: Self {
                events,
                backend: None,
                max_bytes,
                encoded_len: proven_len,
                last_error: None,
                poisoned: false,
            },
            repair_to: (proven_len != bytes.len()).then_some(proven_len),
            input_len: bytes.len(),
        })
    }

    /// Attach a backend whose durable bytes are already validated at this journal boundary.
    #[must_use]
    pub fn with_backend(mut self, backend: impl JournalBackend + 'static) -> Self {
        self.backend = Some(Box::new(backend));
        self
    }

    /// Current proven ADEJ byte length.
    #[must_use]
    pub const fn encoded_len(&self) -> usize {
        self.encoded_len
    }

    /// Validate and prepare an append without changing events, length, or backend state.
    ///
    /// # Errors
    /// Returns a typed record or bound error.
    pub fn prepare_append(
        &self,
        event: JournalEvent,
    ) -> Result<PreparedJournalAppend, JournalError> {
        if self.poisoned {
            return Err(JournalError::Poisoned);
        }
        if matches!(
            &event,
            JournalEvent::TransientWriteApplied { field, .. }
                if *field != "beeper_off_flags"
        ) {
            return Err(JournalError::InvalidRecord {
                offset: self.encoded_len,
            });
        }
        let payload = encode_event(&event);
        if payload.len() > MAX_RECORD_PAYLOAD_BYTES {
            return Err(JournalError::RecordTooLarge(payload.len() as u32));
        }
        let next_len = self
            .encoded_len
            .checked_add(RECORD_OVERHEAD + payload.len())
            .ok_or(JournalError::Full {
                limit: self.max_bytes,
            })?;
        if next_len > self.max_bytes {
            return Err(JournalError::Full {
                limit: self.max_bytes,
            });
        }
        let mut record_bytes = Vec::with_capacity(RECORD_OVERHEAD + payload.len());
        record_bytes.extend((payload.len() as u32).to_le_bytes());
        record_bytes.extend(&payload);
        record_bytes.extend(checksum(&payload).to_le_bytes());
        Ok(PreparedJournalAppend {
            event,
            record_bytes,
            expected_len: self.encoded_len,
            next_len,
        })
    }

    /// Accept an already durable prepared append into logical state.
    ///
    /// # Errors
    /// Refuses a stale preparation without changing the journal.
    pub fn accept_prepared(&mut self, prepared: PreparedJournalAppend) -> Result<(), JournalError> {
        if self.poisoned {
            return Err(JournalError::Poisoned);
        }
        if prepared.expected_len != self.encoded_len {
            return Err(JournalError::StalePreparedAppend {
                expected: prepared.expected_len,
                actual: self.encoded_len,
            });
        }
        self.events.push(prepared.event);
        self.encoded_len = prepared.next_len;
        self.last_error = None;
        Ok(())
    }

    /// Prepare, durably append through the attached backend, then advance logical state.
    ///
    /// # Errors
    /// A typed validation/backend error. Any backend failure poisons this handle until a
    /// fresh decode/reopen proves the durable boundary again.
    pub fn try_append(&mut self, event: JournalEvent) -> Result<(), JournalError> {
        let prepared = match self.prepare_append(event) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.last_error = Some(error.clone());
                return Err(error);
            }
        };
        if let Some(backend) = &mut self.backend {
            if let Err(error) = backend.append_durable(&prepared) {
                self.poisoned = true;
                self.last_error = Some(error.clone());
                return Err(error);
            }
        }
        self.accept_prepared(prepared)
    }

    /// Append an event through the compatibility API.
    ///
    /// New lifecycle code should use [`Journal::try_append`] and classify a failure. This
    /// method remains for the established public API; a refusal is deterministic and can be
    /// inspected with [`Journal::last_error`].
    pub fn append(&mut self, event: JournalEvent) {
        let _ = self.try_append(event);
    }

    /// The recorded events, in order.
    #[must_use]
    pub fn events(&self) -> &[JournalEvent] {
        &self.events
    }

    /// The last recorded event, if any.
    #[must_use]
    pub fn last(&self) -> Option<&JournalEvent> {
        self.events.last()
    }

    /// The most recent append refusal, if the compatibility API could not record an event.
    #[must_use]
    pub fn last_error(&self) -> Option<&JournalError> {
        self.last_error.as_ref()
    }
}

/// Why an effect-backed journal load, repair, or append was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectJournalError {
    /// The host-I/O coordinator rejected a stale, duplicate, wrong-kind, or overlapping action.
    Boundary(BoundaryError),
    /// The host explicitly failed the storage operation.
    Storage(StorageFailure),
    /// ADEJ bytes or an append violated the journal contract.
    Journal(JournalError),
    /// Append was requested before a load had proven the current bytes and revision.
    NotLoaded,
    /// A second load was requested after this store had already established state.
    AlreadyLoaded,
    /// A successful host commit did not return a revision newer than the compared revision.
    NonAdvancingRevision {
        current: StorageRevision,
        received: StorageRevision,
    },
    /// Internal operation state did not match an accepted coordinator response.
    OperationStateMismatch,
}

impl fmt::Display for EffectJournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for EffectJournalError {}

impl From<BoundaryError> for EffectJournalError {
    fn from(error: BoundaryError) -> Self {
        Self::Boundary(error)
    }
}

impl From<JournalError> for EffectJournalError {
    fn from(error: JournalError) -> Self {
        Self::Journal(error)
    }
}

/// A state transition proven by one accepted storage response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectJournalOutcome {
    /// Load completed and the journal is ready without repair.
    Loaded,
    /// The loaded bytes had only an incomplete final record; the returned CAS effect must
    /// succeed before the repaired journal becomes visible.
    RepairRequired(IoEffect),
    /// A prepared append was committed and then accepted into logical state.
    AppendCommitted,
    /// A proposed incomplete-final-tail repair was committed and is now visible.
    RepairCommitted,
}

struct LoadedEffectJournal {
    journal: Journal,
    bytes: Vec<u8>,
    revision: Option<StorageRevision>,
}

impl fmt::Debug for LoadedEffectJournal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoadedEffectJournal")
            .field("journal", &self.journal)
            .field("byte_len", &self.bytes.len())
            .field("revision", &self.revision)
            .finish()
    }
}

enum PendingJournalOperation {
    Load,
    Append {
        prepared: PreparedJournalAppend,
        bytes: Vec<u8>,
        expected_revision: Option<StorageRevision>,
    },
    Repair {
        journal: Journal,
        bytes: Vec<u8>,
        expected_revision: StorageRevision,
    },
}

impl fmt::Debug for PendingJournalOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load => formatter.write_str("Load"),
            Self::Append {
                prepared,
                bytes,
                expected_revision,
            } => formatter
                .debug_struct("Append")
                .field("prepared", prepared)
                .field("byte_len", &bytes.len())
                .field("expected_revision", expected_revision)
                .finish(),
            Self::Repair {
                journal,
                bytes,
                expected_revision,
            } => formatter
                .debug_struct("Repair")
                .field("journal", journal)
                .field("byte_len", &bytes.len())
                .field("expected_revision", expected_revision)
                .finish(),
        }
    }
}

/// Backend-neutral ADEJ storage driven through typed load/CAS host effects.
///
/// The store never invents a revision. A prepared append or proposed torn-tail repair becomes
/// visible only after an exact matching host response proves CAS success.
#[derive(Debug)]
pub struct EffectJournalStore {
    key: StorageKey,
    max_bytes: usize,
    coordinator: IoCoordinator,
    loaded: Option<LoadedEffectJournal>,
    pending: Option<PendingJournalOperation>,
}

impl EffectJournalStore {
    /// Create an unloaded effect-backed journal store.
    ///
    /// # Errors
    /// Returns [`JournalError::LimitTooSmall`] through [`EffectJournalError`] before any effect.
    pub fn new(key: StorageKey, max_bytes: usize) -> Result<Self, EffectJournalError> {
        Journal::with_limit(max_bytes)?;
        Ok(Self {
            key,
            max_bytes,
            coordinator: IoCoordinator::new(),
            loaded: None,
            pending: None,
        })
    }

    /// Emit a typed load effect. The loaded state changes only when its response is accepted.
    ///
    /// # Errors
    /// Refuses overlapping work or a second load after state has been established.
    pub fn begin_load(&mut self) -> Result<IoEffect, EffectJournalError> {
        if self.loaded.is_some() {
            return Err(EffectJournalError::AlreadyLoaded);
        }
        let effect = self.coordinator.begin_storage(StorageEffect::Load {
            key: self.key.clone(),
        })?;
        self.pending = Some(PendingJournalOperation::Load);
        Ok(effect)
    }

    /// Prepare an event and emit a full-value compare-and-swap effect.
    ///
    /// PREPARED is not DURABLY ACCEPTED: events, bytes, and revision remain unchanged until
    /// [`Self::accept_response`] receives a matching successful commit response.
    ///
    /// # Errors
    /// Refuses an unloaded store, overlapping work, or an invalid/full append.
    pub fn begin_append(&mut self, event: JournalEvent) -> Result<IoEffect, EffectJournalError> {
        let loaded = self.loaded.as_ref().ok_or(EffectJournalError::NotLoaded)?;
        let prepared = loaded.journal.prepare_append(event)?;
        let mut bytes = loaded.bytes.clone();
        bytes.extend(prepared.record_bytes());
        let expected_revision = loaded.revision;
        let effect = self
            .coordinator
            .begin_storage(StorageEffect::CompareAndSwap {
                key: self.key.clone(),
                expected_revision,
                bytes: bytes.clone(),
            })?;
        self.pending = Some(PendingJournalOperation::Append {
            prepared,
            bytes,
            expected_revision,
        });
        Ok(effect)
    }

    /// Validate one asynchronous host response and apply only a proven transition.
    ///
    /// Stale, duplicated and wrong-kind responses do not consume pending work. Host failures
    /// consume the matching operation but leave journal bytes and revision unchanged.
    ///
    /// # Errors
    /// Returns a typed boundary, storage, journal, or revision-honesty refusal.
    pub fn accept_response(
        &mut self,
        response: IoResponse,
    ) -> Result<EffectJournalOutcome, EffectJournalError> {
        let accepted = self.coordinator.accept(response)?;
        let pending = self
            .pending
            .take()
            .ok_or(EffectJournalError::OperationStateMismatch)?;
        match (pending, accepted) {
            (
                PendingJournalOperation::Load,
                IoResponse::Storage {
                    result: StorageResult::Load(result),
                    ..
                },
            ) => self.accept_load_result(result),
            (
                PendingJournalOperation::Append {
                    prepared,
                    bytes,
                    expected_revision,
                },
                IoResponse::Storage {
                    result: StorageResult::Commit(result),
                    ..
                },
            ) => {
                let revision = result.map_err(EffectJournalError::Storage)?;
                ensure_revision_advanced(expected_revision, revision)?;
                let loaded = self
                    .loaded
                    .as_mut()
                    .ok_or(EffectJournalError::OperationStateMismatch)?;
                loaded.journal.accept_prepared(prepared)?;
                loaded.bytes = bytes;
                loaded.revision = Some(revision);
                Ok(EffectJournalOutcome::AppendCommitted)
            }
            (
                PendingJournalOperation::Repair {
                    journal,
                    bytes,
                    expected_revision,
                },
                IoResponse::Storage {
                    result: StorageResult::Commit(result),
                    ..
                },
            ) => {
                let revision = result.map_err(EffectJournalError::Storage)?;
                ensure_revision_advanced(Some(expected_revision), revision)?;
                self.loaded = Some(LoadedEffectJournal {
                    journal,
                    bytes,
                    revision: Some(revision),
                });
                Ok(EffectJournalOutcome::RepairCommitted)
            }
            _ => Err(EffectJournalError::OperationStateMismatch),
        }
    }

    fn accept_load_result(
        &mut self,
        result: Result<Option<StoredValue>, StorageFailure>,
    ) -> Result<EffectJournalOutcome, EffectJournalError> {
        let value = result.map_err(EffectJournalError::Storage)?;
        let Some(value) = value else {
            self.loaded = Some(LoadedEffectJournal {
                journal: Journal::with_limit(self.max_bytes)?,
                bytes: empty_journal_bytes().to_vec(),
                revision: None,
            });
            return Ok(EffectJournalOutcome::Loaded);
        };

        let decoded = Journal::decode(&value.bytes, self.max_bytes)?;
        if let Some(repair_to) = decoded.repair_to() {
            let bytes = value.bytes[..repair_to].to_vec();
            let journal = decoded.into_journal();
            let effect = self
                .coordinator
                .begin_storage(StorageEffect::CompareAndSwap {
                    key: self.key.clone(),
                    expected_revision: Some(value.revision),
                    bytes: bytes.clone(),
                })?;
            self.pending = Some(PendingJournalOperation::Repair {
                journal,
                bytes,
                expected_revision: value.revision,
            });
            return Ok(EffectJournalOutcome::RepairRequired(effect));
        }

        self.loaded = Some(LoadedEffectJournal {
            journal: decoded.into_journal(),
            bytes: value.bytes,
            revision: Some(value.revision),
        });
        Ok(EffectJournalOutcome::Loaded)
    }

    /// The proven journal, unavailable before a clean load or successful repair CAS.
    #[must_use]
    pub fn journal(&self) -> Option<&Journal> {
        self.loaded.as_ref().map(|loaded| &loaded.journal)
    }

    /// The exact revision supplied by the last successful host load/commit.
    #[must_use]
    pub fn revision(&self) -> Option<StorageRevision> {
        self.loaded.as_ref().and_then(|loaded| loaded.revision)
    }

    /// Whether the coordinator still expects a storage response.
    #[must_use]
    pub const fn has_pending(&self) -> bool {
        self.coordinator.has_pending_storage()
    }
}

fn ensure_revision_advanced(
    current: Option<StorageRevision>,
    received: StorageRevision,
) -> Result<(), EffectJournalError> {
    if let Some(current) = current {
        if received.get() <= current.get() {
            return Err(EffectJournalError::NonAdvancingRevision { current, received });
        }
    }
    Ok(())
}

/// What to do when a process restart is detected mid-case, per the transient-write contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileDecision {
    /// Nothing was in flight; the case may continue from where it stopped.
    Continue,
    /// A transient write was applied but not saved: re-read to determine the real state.
    /// This is `TRANSIENT_WRITE_PENDING — RECONCILE_ON_RESUME`, never a silent rollback.
    ReconcileTransientWrite,
    /// A save was in flight when interrupted: verify before assuming success or failure.
    VerifySaveOutcome,
    /// The journal already reached a terminal state.
    AlreadyTerminal,
    /// Recovery itself was interrupted after durable write-ahead evidence; no safe automatic
    /// continuation can be inferred from the journal alone.
    StateUnknown,
}

/// Decide how to resume from a journal after a restart. This never assumes success and never
/// silently rolls back a transient write.
#[must_use]
pub fn reconcile_on_resume(journal: &Journal) -> ReconcileDecision {
    if matches!(
        journal.last(),
        Some(JournalEvent::Verified | JournalEvent::Restored | JournalEvent::StateUnknown)
    ) {
        return ReconcileDecision::AlreadyTerminal;
    }
    // Once recovery starts, only its own verified terminal marker can make the journal
    // resumable. Events such as `Saved` and `Rebooted` are deliberately shared with the
    // normal path, so looking at the last event alone could otherwise mistake an interrupted
    // restore for the original apply path.
    if journal
        .events()
        .iter()
        .any(|event| matches!(event, JournalEvent::RecoveryStarted))
    {
        return ReconcileDecision::StateUnknown;
    }
    match journal.last() {
        None => ReconcileDecision::Continue,
        Some(event) => match event {
            JournalEvent::TransientWriteApplied { .. } => {
                ReconcileDecision::ReconcileTransientWrite
            }
            JournalEvent::Saved => ReconcileDecision::VerifySaveOutcome,
            JournalEvent::WriteAhead { class, recovery } => match (class, recovery) {
                (
                    WriteCommandClass::TransientConfig,
                    RecoveryClass::TransientWritePendingReconcileOnResume,
                ) => ReconcileDecision::ReconcileTransientWrite,
                (WriteCommandClass::PersistentConfig | WriteCommandClass::Reboot, _)
                    if *recovery != RecoveryClass::RestoreFromBackupSupported =>
                {
                    ReconcileDecision::VerifySaveOutcome
                }
                _ => ReconcileDecision::StateUnknown,
            },
            JournalEvent::Verified | JournalEvent::Restored | JournalEvent::StateUnknown => {
                unreachable!("terminal events returned above")
            }
            _ => ReconcileDecision::Continue,
        },
    }
}

/// The outcome recorded for a case's verification step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationOutcome {
    /// The intended bit changed and the DShot fields were unchanged.
    IntendedBitOnly,
    /// Verification failed.
    Failed,
    /// Verification could not be performed.
    NotPerformed,
}

/// The outcome recorded for a case's recovery step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryOutcome {
    /// Not needed.
    NotRequired,
    /// The previous value was restored and verified.
    Restored,
    /// The state could not be proven.
    StateUnknown,
}

/// A local, versioned case record. Contains no hardware-derived or tracking identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseRecord {
    /// Schema version.
    pub schema_version: u32,
    /// A host-supplied, non-hardware-derived case identifier.
    pub case_id: String,
    /// A host-supplied label for the start time (no wall clock is read here).
    pub started_at_label: String,
    /// A host-supplied label for the end time, once finished.
    pub ended_at_label: Option<String>,
    /// The simulation target this case ran against.
    pub execution_target: ExecutionTarget,
    /// The identity observed at the start, if identification succeeded.
    pub initial_identity: Option<DeviceIdentity>,
    /// The classes of the outbound commands, in order.
    pub outbound_classes: Vec<WriteCommandClass>,
    /// The recovery class declared for the write.
    pub recovery_class: RecoveryClass,
    /// The verification outcome.
    pub verification: VerificationOutcome,
    /// The recovery outcome.
    pub recovery: RecoveryOutcome,
    /// The named terminal readiness/state.
    pub terminal_state: Option<String>,
}

impl CaseRecord {
    /// Start a new case record. `case_id` must not be derived from the device.
    #[must_use]
    pub fn start(
        case_id: impl Into<String>,
        started_at_label: impl Into<String>,
        execution_target: ExecutionTarget,
    ) -> Self {
        Self {
            schema_version: CASE_SCHEMA_VERSION,
            case_id: case_id.into(),
            started_at_label: started_at_label.into(),
            ended_at_label: None,
            execution_target,
            initial_identity: None,
            outbound_classes: Vec::new(),
            recovery_class: RecoveryClass::NotApplicableNoWrite,
            verification: VerificationOutcome::NotPerformed,
            recovery: RecoveryOutcome::NotRequired,
            terminal_state: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Debug)]
    struct MemoryBackend {
        bytes: Rc<RefCell<Vec<u8>>>,
    }

    impl JournalBackend for MemoryBackend {
        fn append_durable(&mut self, prepared: &PreparedJournalAppend) -> Result<(), JournalError> {
            let mut bytes = self.bytes.borrow_mut();
            if bytes.len() != prepared.expected_len() {
                return Err(JournalError::BackendPositionMismatch {
                    expected: prepared.expected_len(),
                    actual: bytes.len(),
                });
            }
            bytes.extend(prepared.record_bytes());
            Ok(())
        }
    }

    fn memory_journal(max_bytes: usize) -> (Journal, Rc<RefCell<Vec<u8>>>) {
        let bytes = Rc::new(RefCell::new(empty_journal_bytes().to_vec()));
        let journal = Journal::with_limit(max_bytes)
            .unwrap()
            .with_backend(MemoryBackend {
                bytes: Rc::clone(&bytes),
            });
        (journal, bytes)
    }

    fn effect_key() -> StorageKey {
        StorageKey::new("case-effect-0001").unwrap()
    }

    fn journal_bytes(events: &[JournalEvent]) -> Vec<u8> {
        let (mut journal, bytes) = memory_journal(4096);
        for event in events {
            journal.try_append(event.clone()).unwrap();
        }
        drop(journal);
        Rc::try_unwrap(bytes).unwrap().into_inner()
    }

    fn load_response(
        effect: &IoEffect,
        result: Result<Option<StoredValue>, StorageFailure>,
    ) -> IoResponse {
        IoResponse::Storage {
            request_id: effect.request_id(),
            result: StorageResult::Load(result),
        }
    }

    fn commit_response(
        effect: &IoEffect,
        result: Result<StorageRevision, StorageFailure>,
    ) -> IoResponse {
        IoResponse::Storage {
            request_id: effect.request_id(),
            result: StorageResult::Commit(result),
        }
    }

    fn loaded_effect_store(revision: u64) -> EffectJournalStore {
        let mut store = EffectJournalStore::new(effect_key(), 4096).unwrap();
        let load = store.begin_load().unwrap();
        assert_eq!(
            store
                .accept_response(load_response(
                    &load,
                    Ok(Some(StoredValue {
                        revision: StorageRevision::new(revision),
                        bytes: journal_bytes(&[JournalEvent::IdentityRead]),
                    })),
                ))
                .unwrap(),
            EffectJournalOutcome::Loaded
        );
        store
    }

    fn begin_torn_repair(store: &mut EffectJournalStore) -> IoEffect {
        let mut bytes = journal_bytes(&[JournalEvent::IdentityRead]);
        bytes.extend([4, 0, 0, 0, 9]);
        let load = store.begin_load().unwrap();
        let outcome = store
            .accept_response(load_response(
                &load,
                Ok(Some(StoredValue {
                    revision: StorageRevision::new(4),
                    bytes,
                })),
            ))
            .unwrap();
        let EffectJournalOutcome::RepairRequired(effect) = outcome else {
            panic!("torn final tail must require a CAS repair");
        };
        assert!(store.journal().is_none());
        assert_eq!(store.revision(), None);
        effect
    }

    #[test]
    fn effect_journal_load_accepts_host_bytes_and_revision_together() {
        let store = loaded_effect_store(3);
        assert_eq!(
            store.journal().unwrap().events(),
            &[JournalEvent::IdentityRead]
        );
        assert_eq!(store.revision(), Some(StorageRevision::new(3)));
        assert!(!store.has_pending());
    }

    #[test]
    fn effect_journal_append_advances_only_after_cas_success() {
        let mut store = EffectJournalStore::new(effect_key(), 4096).unwrap();
        let load = store.begin_load().unwrap();
        assert_eq!(
            store
                .accept_response(load_response(&load, Ok(None)))
                .unwrap(),
            EffectJournalOutcome::Loaded
        );
        let append = store.begin_append(JournalEvent::SnapshotRead).unwrap();
        match &append {
            IoEffect::Storage {
                effect:
                    StorageEffect::CompareAndSwap {
                        expected_revision,
                        bytes,
                        ..
                    },
                ..
            } => {
                assert_eq!(*expected_revision, None);
                assert!(bytes.len() > JOURNAL_HEADER_LEN);
            }
            _ => panic!("append must emit storage CAS"),
        }
        assert!(store.journal().unwrap().events().is_empty());
        assert_eq!(store.revision(), None);

        assert_eq!(
            store
                .accept_response(commit_response(&append, Ok(StorageRevision::new(1)),))
                .unwrap(),
            EffectJournalOutcome::AppendCommitted
        );
        assert_eq!(
            store.journal().unwrap().events(),
            &[JournalEvent::SnapshotRead]
        );
        assert_eq!(store.revision(), Some(StorageRevision::new(1)));
    }

    #[test]
    fn every_host_append_failure_preserves_journal_and_revision() {
        for failure in [
            StorageFailure::Conflict,
            StorageFailure::QuotaExceeded,
            StorageFailure::Unavailable,
            StorageFailure::Cancelled,
            StorageFailure::Corrupt,
            StorageFailure::Unknown,
        ] {
            let mut store = loaded_effect_store(4);
            let before_events = store.journal().unwrap().events().to_vec();
            let before_revision = store.revision();
            let append = store.begin_append(JournalEvent::SnapshotRead).unwrap();
            assert_eq!(
                store.accept_response(commit_response(&append, Err(failure))),
                Err(EffectJournalError::Storage(failure))
            );
            assert_eq!(store.journal().unwrap().events(), before_events);
            assert_eq!(store.revision(), before_revision);
            assert!(!store.has_pending());
        }
    }

    #[test]
    fn stale_append_response_is_rejected_without_consuming_pending_or_state() {
        let mut store = loaded_effect_store(4);
        let append = store.begin_append(JournalEvent::SnapshotRead).unwrap();
        let expected = append.request_id();
        let stale = ade_runtime_ports::RequestId::new(expected.get() + 1);
        assert_eq!(
            store.accept_response(IoResponse::Storage {
                request_id: stale,
                result: StorageResult::Commit(Ok(StorageRevision::new(5))),
            }),
            Err(EffectJournalError::Boundary(
                BoundaryError::RequestIdMismatch {
                    expected,
                    received: stale,
                }
            ))
        );
        assert_eq!(
            store.journal().unwrap().events(),
            &[JournalEvent::IdentityRead]
        );
        assert_eq!(store.revision(), Some(StorageRevision::new(4)));
        assert!(store.has_pending());
        assert_eq!(
            store.accept_response(commit_response(&append, Err(StorageFailure::Conflict),)),
            Err(EffectJournalError::Storage(StorageFailure::Conflict))
        );
    }

    #[test]
    fn duplicate_append_response_cannot_advance_twice() {
        let mut store = loaded_effect_store(4);
        let append = store.begin_append(JournalEvent::SnapshotRead).unwrap();
        let response = commit_response(&append, Ok(StorageRevision::new(5)));
        assert_eq!(
            store.accept_response(response.clone()).unwrap(),
            EffectJournalOutcome::AppendCommitted
        );
        let events = store.journal().unwrap().events().to_vec();
        let revision = store.revision();
        assert_eq!(
            store.accept_response(response),
            Err(EffectJournalError::Boundary(
                BoundaryError::NoStorageRequestPending
            ))
        );
        assert_eq!(store.journal().unwrap().events(), events);
        assert_eq!(store.revision(), revision);
    }

    #[test]
    fn wrong_kind_append_response_is_rejected_without_consuming_pending() {
        let mut store = loaded_effect_store(4);
        let append = store.begin_append(JournalEvent::SnapshotRead).unwrap();
        assert_eq!(
            store.accept_response(IoResponse::Storage {
                request_id: append.request_id(),
                result: StorageResult::Load(Ok(None)),
            }),
            Err(EffectJournalError::Boundary(
                BoundaryError::ResponseKindMismatch
            ))
        );
        assert_eq!(
            store.journal().unwrap().events(),
            &[JournalEvent::IdentityRead]
        );
        assert_eq!(store.revision(), Some(StorageRevision::new(4)));
        assert!(store.has_pending());
        assert_eq!(
            store.accept_response(commit_response(&append, Err(StorageFailure::Cancelled),)),
            Err(EffectJournalError::Storage(StorageFailure::Cancelled))
        );
    }

    #[test]
    fn non_advancing_host_revision_is_not_accepted_as_storage_success() {
        let mut store = loaded_effect_store(4);
        let append = store.begin_append(JournalEvent::SnapshotRead).unwrap();
        assert_eq!(
            store.accept_response(commit_response(&append, Ok(StorageRevision::new(4)),)),
            Err(EffectJournalError::NonAdvancingRevision {
                current: StorageRevision::new(4),
                received: StorageRevision::new(4),
            })
        );
        assert_eq!(
            store.journal().unwrap().events(),
            &[JournalEvent::IdentityRead]
        );
        assert_eq!(store.revision(), Some(StorageRevision::new(4)));
    }

    #[test]
    fn torn_final_tail_becomes_visible_only_after_repair_cas_success() {
        let mut store = EffectJournalStore::new(effect_key(), 4096).unwrap();
        let repair = begin_torn_repair(&mut store);
        match &repair {
            IoEffect::Storage {
                effect:
                    StorageEffect::CompareAndSwap {
                        expected_revision,
                        bytes,
                        ..
                    },
                ..
            } => {
                assert_eq!(*expected_revision, Some(StorageRevision::new(4)));
                assert_eq!(
                    Journal::decode(bytes, 4096)
                        .unwrap()
                        .into_journal()
                        .events(),
                    &[JournalEvent::IdentityRead]
                );
            }
            _ => panic!("repair must emit storage CAS"),
        }
        assert_eq!(
            store
                .accept_response(commit_response(&repair, Ok(StorageRevision::new(5)),))
                .unwrap(),
            EffectJournalOutcome::RepairCommitted
        );
        assert_eq!(
            store.journal().unwrap().events(),
            &[JournalEvent::IdentityRead]
        );
        assert_eq!(store.revision(), Some(StorageRevision::new(5)));
    }

    #[test]
    fn repair_conflict_and_cancel_leave_journal_and_revision_unestablished() {
        for failure in [StorageFailure::Conflict, StorageFailure::Cancelled] {
            let mut store = EffectJournalStore::new(effect_key(), 4096).unwrap();
            let repair = begin_torn_repair(&mut store);
            assert_eq!(
                store.accept_response(commit_response(&repair, Err(failure))),
                Err(EffectJournalError::Storage(failure))
            );
            assert!(store.journal().is_none());
            assert_eq!(store.revision(), None);
            assert!(!store.has_pending());
            assert!(
                store.begin_load().is_ok(),
                "failed repair must require reload"
            );
        }
    }

    #[test]
    fn effect_journal_debug_redacts_storage_keys_and_raw_bytes() {
        let mut store = loaded_effect_store(4);
        let append = store.begin_append(JournalEvent::SnapshotRead).unwrap();
        let debug = format!("{store:?}");
        assert!(!debug.contains(effect_key().as_str()));
        assert!(!debug.contains("bytes: ["));
        assert!(!debug.contains("record_bytes"));
        assert!(debug.contains("byte_len"));

        assert_eq!(
            store.accept_response(commit_response(&append, Err(StorageFailure::Cancelled),)),
            Err(EffectJournalError::Storage(StorageFailure::Cancelled))
        );
    }

    #[test]
    fn a_transient_write_without_a_save_reconciles_on_resume() {
        let mut journal = Journal::new();
        journal.append(JournalEvent::Started {
            execution_target: ExecutionTarget::Mock,
        });
        journal.append(JournalEvent::IdentityRead);
        journal.append(JournalEvent::BackedUp);
        journal.append(JournalEvent::TransientWriteApplied {
            field: "beeper_off_flags",
            mask: 0x0001_0000,
        });
        assert_eq!(
            reconcile_on_resume(&journal),
            ReconcileDecision::ReconcileTransientWrite
        );
    }

    #[test]
    fn a_save_in_flight_requires_verification_on_resume() {
        let mut journal = Journal::new();
        journal.append(JournalEvent::Saved);
        assert_eq!(
            reconcile_on_resume(&journal),
            ReconcileDecision::VerifySaveOutcome
        );
    }

    #[test]
    fn a_terminal_journal_is_recognised() {
        let mut journal = Journal::new();
        journal.append(JournalEvent::Verified);
        assert_eq!(
            reconcile_on_resume(&journal),
            ReconcileDecision::AlreadyTerminal
        );
    }

    #[test]
    fn a_case_record_has_no_hardware_derived_identifier_field() {
        let record = CaseRecord::start("case-0001", "t0", ExecutionTarget::Mock);
        // The case id is host-supplied and not derived from the device.
        assert_eq!(record.case_id, "case-0001");
        assert_eq!(record.schema_version, CASE_SCHEMA_VERSION);
    }

    #[test]
    fn durable_journal_has_a_stable_golden_prefix_and_round_trips() {
        let (mut journal, bytes) = memory_journal(1024);
        journal.try_append(JournalEvent::IdentityRead).unwrap();
        journal
            .try_append(JournalEvent::WriteAhead {
                class: WriteCommandClass::PersistentConfig,
                recovery: RecoveryClass::AutomaticRollbackSupported,
            })
            .unwrap();
        let bytes = bytes.borrow().clone();
        assert_eq!(
            &bytes[..17],
            &[
                b'A', b'D', b'E', b'J', 1, 0, 0, 0, // v1 header
                1, 0, 0, 0, 2, // IdentityRead record
                0x45, 0x60, 0x0c, 0x07, // FNV-1a checksum
            ]
        );
        let reopened = Journal::decode(&bytes, 1024).unwrap().into_journal();
        assert_eq!(
            reopened.events(),
            &[
                JournalEvent::IdentityRead,
                JournalEvent::WriteAhead {
                    class: WriteCommandClass::PersistentConfig,
                    recovery: RecoveryClass::AutomaticRollbackSupported,
                },
            ]
        );
    }

    #[test]
    fn binary_codec_round_trips_every_trusted_discriminant() {
        let events = vec![
            JournalEvent::Started {
                execution_target: ExecutionTarget::Mock,
            },
            JournalEvent::Started {
                execution_target: ExecutionTarget::Replay,
            },
            JournalEvent::Started {
                execution_target: ExecutionTarget::Hardware,
            },
            JournalEvent::IdentityRead,
            JournalEvent::SnapshotRead,
            JournalEvent::BackedUp,
            JournalEvent::TransientWriteApplied {
                field: "beeper_off_flags",
                mask: 0xa5a5_5a5a,
            },
            JournalEvent::ReReadBeforeSave,
            JournalEvent::Saved,
            JournalEvent::Rebooted,
            JournalEvent::Reconnected,
            JournalEvent::Verified,
            JournalEvent::RecoveryStarted,
            JournalEvent::Restored,
            JournalEvent::StateUnknown,
            JournalEvent::WriteAhead {
                class: WriteCommandClass::NoWrite,
                recovery: RecoveryClass::NotApplicableNoWrite,
            },
            JournalEvent::WriteAhead {
                class: WriteCommandClass::TransientConfig,
                recovery: RecoveryClass::TransientWritePendingReconcileOnResume,
            },
            JournalEvent::WriteAhead {
                class: WriteCommandClass::PersistentConfig,
                recovery: RecoveryClass::AutomaticRollbackSupported,
            },
            JournalEvent::WriteAhead {
                class: WriteCommandClass::Reboot,
                recovery: RecoveryClass::RestoreFromBackupSupported,
            },
            JournalEvent::WriteAhead {
                class: WriteCommandClass::TransientConfig,
                recovery: RecoveryClass::ManualRecoveryRequired,
            },
            JournalEvent::WriteAhead {
                class: WriteCommandClass::PersistentConfig,
                recovery: RecoveryClass::StateUnknownRecoveryRequired,
            },
        ];
        for event in events {
            assert_eq!(decode_event(&encode_event(&event)), Some(event));
        }

        assert_eq!(decode_event(&[1, u8::MAX]), None);
        assert_eq!(decode_event(&[14, u8::MAX, 0]), None);
        assert_eq!(decode_event(&[14, 0, u8::MAX]), None);
        assert_eq!(decode_event(&[u8::MAX]), None);
        assert_eq!(
            decode_event(&encode_event(&JournalEvent::TransientWriteApplied {
                field: "untrusted_field",
                mask: 0,
            })),
            None
        );
    }

    #[test]
    fn format_bounds_and_untrusted_events_fail_closed() {
        assert_eq!(
            Journal::with_limit(HEADER_LEN - 1).unwrap_err(),
            JournalError::LimitTooSmall
        );

        assert_eq!(
            Journal::decode(&header(), HEADER_LEN - 1).unwrap_err(),
            JournalError::LimitTooSmall
        );

        let mut oversized_bytes = header().to_vec();
        oversized_bytes.push(0);
        assert_eq!(
            Journal::decode(&oversized_bytes, HEADER_LEN).unwrap_err(),
            JournalError::Full { limit: HEADER_LEN }
        );

        let mut invalid_header_bytes = header();
        invalid_header_bytes[6] = 1;
        assert_eq!(
            Journal::decode(&invalid_header_bytes, 1024).unwrap_err(),
            JournalError::InvalidHeader
        );

        let payload = [u8::MAX];
        let mut invalid_record_bytes = header().to_vec();
        invalid_record_bytes.extend((payload.len() as u32).to_le_bytes());
        invalid_record_bytes.extend(payload);
        invalid_record_bytes.extend(checksum(&payload).to_le_bytes());
        assert_eq!(
            Journal::decode(&invalid_record_bytes, 1024).unwrap_err(),
            JournalError::InvalidRecord { offset: HEADER_LEN }
        );

        assert_eq!(
            Journal::decode(&[], 1024).unwrap_err(),
            JournalError::InvalidMagic
        );

        let mut journal = Journal::new();
        let expected = JournalError::InvalidRecord { offset: HEADER_LEN };
        assert_eq!(
            journal.try_append(JournalEvent::TransientWriteApplied {
                field: "untrusted_field",
                mask: 0,
            }),
            Err(expected.clone())
        );
        assert_eq!(journal.last_error(), Some(&expected));
        assert!(journal.events().is_empty());
    }

    #[test]
    fn torn_tail_is_proposed_without_mutating_decoded_state() {
        let (mut journal, bytes) = memory_journal(1024);
        journal.try_append(JournalEvent::IdentityRead).unwrap();
        let proven_len = journal.encoded_len();
        let mut torn = bytes.borrow().clone();
        torn.extend([6, 0, 0, 0, 5, 1]);

        let decoded = Journal::decode(&torn, 1024).unwrap();
        assert_eq!(decoded.repair_to(), Some(proven_len));
        assert_eq!(decoded.input_len(), torn.len());
        assert_eq!(
            decoded.into_journal().events(),
            &[JournalEvent::IdentityRead]
        );
    }

    #[test]
    fn complete_checksum_corruption_is_rejected_not_truncated() {
        let (mut journal, bytes) = memory_journal(1024);
        journal.try_append(JournalEvent::IdentityRead).unwrap();
        let mut corrupt = bytes.borrow().clone();
        corrupt[HEADER_LEN + 4] = 3;
        assert_eq!(
            Journal::decode(&corrupt, 1024).unwrap_err(),
            JournalError::ChecksumMismatch { offset: HEADER_LEN }
        );
    }

    #[test]
    fn prepared_append_does_not_advance_and_stale_acceptance_is_refused() {
        let mut journal = Journal::new();
        let first = journal.prepare_append(JournalEvent::IdentityRead).unwrap();
        let stale = journal.prepare_append(JournalEvent::SnapshotRead).unwrap();
        assert!(journal.events().is_empty());
        assert_eq!(journal.encoded_len(), HEADER_LEN);
        journal.accept_prepared(first).unwrap();
        assert_eq!(
            journal.accept_prepared(stale),
            Err(JournalError::StalePreparedAppend {
                expected: HEADER_LEN,
                actual: journal.encoded_len(),
            })
        );
        assert_eq!(journal.events(), &[JournalEvent::IdentityRead]);
    }

    #[test]
    fn journal_bound_refuses_the_next_record_without_mutating_order() {
        let mut journal = Journal::with_limit(HEADER_LEN + RECORD_OVERHEAD + 1).unwrap();
        journal.try_append(JournalEvent::IdentityRead).unwrap();
        assert_eq!(
            journal.try_append(JournalEvent::SnapshotRead),
            Err(JournalError::Full {
                limit: HEADER_LEN + RECORD_OVERHEAD + 1,
            })
        );
        assert_eq!(journal.events(), &[JournalEvent::IdentityRead]);
    }

    #[test]
    fn invalid_magic_version_and_oversized_record_are_fail_closed() {
        for bytes_and_expected in [
            (
                vec![b'B', b'A', b'D', b'!', 1, 0, 0, 0],
                JournalError::InvalidMagic,
            ),
            (
                vec![b'A', b'D', b'E', b'J', 2, 0, 0, 0],
                JournalError::UnsupportedVersion(2),
            ),
            (
                {
                    let mut bytes = header().to_vec();
                    bytes.extend(((MAX_RECORD_PAYLOAD_BYTES + 1) as u32).to_le_bytes());
                    bytes
                },
                JournalError::RecordTooLarge((MAX_RECORD_PAYLOAD_BYTES + 1) as u32),
            ),
        ] {
            let (bytes, expected) = bytes_and_expected;
            assert_eq!(Journal::decode(&bytes, 1024).unwrap_err(), expected);
        }
    }

    #[test]
    fn write_ahead_drives_conservative_resume_decisions() {
        let mut transient = Journal::new();
        transient.append(JournalEvent::WriteAhead {
            class: WriteCommandClass::TransientConfig,
            recovery: RecoveryClass::TransientWritePendingReconcileOnResume,
        });
        assert_eq!(
            reconcile_on_resume(&transient),
            ReconcileDecision::ReconcileTransientWrite
        );

        let mut save = Journal::new();
        save.append(JournalEvent::WriteAhead {
            class: WriteCommandClass::PersistentConfig,
            recovery: RecoveryClass::AutomaticRollbackSupported,
        });
        assert_eq!(
            reconcile_on_resume(&save),
            ReconcileDecision::VerifySaveOutcome
        );

        let mut recovery = Journal::new();
        recovery.append(JournalEvent::WriteAhead {
            class: WriteCommandClass::TransientConfig,
            recovery: RecoveryClass::RestoreFromBackupSupported,
        });
        assert_eq!(
            reconcile_on_resume(&recovery),
            ReconcileDecision::StateUnknown
        );
    }

    #[test]
    fn every_interrupted_recovery_stage_stays_state_unknown() {
        for tail in [
            vec![JournalEvent::RecoveryStarted],
            vec![
                JournalEvent::RecoveryStarted,
                JournalEvent::WriteAhead {
                    class: WriteCommandClass::TransientConfig,
                    recovery: RecoveryClass::RestoreFromBackupSupported,
                },
                JournalEvent::TransientWriteApplied {
                    field: "beeper_off_flags",
                    mask: 0x0001_0000,
                },
            ],
            vec![
                JournalEvent::RecoveryStarted,
                JournalEvent::WriteAhead {
                    class: WriteCommandClass::PersistentConfig,
                    recovery: RecoveryClass::RestoreFromBackupSupported,
                },
                JournalEvent::Saved,
            ],
            vec![
                JournalEvent::RecoveryStarted,
                JournalEvent::WriteAhead {
                    class: WriteCommandClass::Reboot,
                    recovery: RecoveryClass::ManualRecoveryRequired,
                },
                JournalEvent::Rebooted,
            ],
        ] {
            let mut journal = Journal::new();
            for event in tail {
                journal.append(event);
            }
            assert_eq!(
                reconcile_on_resume(&journal),
                ReconcileDecision::StateUnknown
            );
        }

        let mut complete = Journal::new();
        complete.append(JournalEvent::RecoveryStarted);
        complete.append(JournalEvent::Restored);
        assert_eq!(
            reconcile_on_resume(&complete),
            ReconcileDecision::AlreadyTerminal
        );
    }

    #[test]
    fn a_backend_failure_does_not_advance_and_poisons_the_handle() {
        #[derive(Debug)]
        struct FailingBackend;
        impl JournalBackend for FailingBackend {
            fn append_durable(
                &mut self,
                _prepared: &PreparedJournalAppend,
            ) -> Result<(), JournalError> {
                Err(JournalError::Io(ErrorKind::PermissionDenied))
            }
        }

        let mut journal = Journal::new().with_backend(FailingBackend);
        assert_eq!(
            journal.try_append(JournalEvent::IdentityRead),
            Err(JournalError::Io(ErrorKind::PermissionDenied))
        );
        assert_eq!(
            journal.try_append(JournalEvent::SnapshotRead),
            Err(JournalError::Poisoned)
        );
        let prepared_before_failure = Journal::new()
            .prepare_append(JournalEvent::SnapshotRead)
            .unwrap();
        assert_eq!(
            journal.accept_prepared(prepared_before_failure),
            Err(JournalError::Poisoned)
        );
        assert!(journal.events().is_empty());
        assert_eq!(journal.encoded_len(), HEADER_LEN);
    }
}
