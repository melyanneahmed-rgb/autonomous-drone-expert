#![forbid(unsafe_code)]

//! # `ade-casebook` — local case journal and resume reconciliation (M1)
//!
//! A local-only, append-only [`Journal`] of lifecycle events, from which the orchestrator
//! can be deterministically reconstructed after a process restart. The [`CaseRecord`] schema
//! is versioned and, by construction, carries **no** hardware-derived or tracking identifier:
//! the case id is supplied by the host and must not be derived from the device.

use ade_facts::DeviceIdentity;
use ade_safety::{ExecutionTarget, RecoveryClass, WriteCommandClass};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const JOURNAL_MAGIC: &[u8; 4] = b"ADEJ";
const JOURNAL_VERSION: u16 = 1;
const HEADER_LEN: usize = 8;
const RECORD_OVERHEAD: usize = 8;
const MAX_RECORD_PAYLOAD_BYTES: usize = 64;

/// Default upper bound for a journal, including its header and record framing.
pub const DEFAULT_MAX_JOURNAL_BYTES: usize = 64 * 1024;

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

#[derive(Debug)]
struct DurableStorage {
    path: PathBuf,
    file: File,
}

/// An append-only journal of lifecycle events (local only, never uploaded).
///
/// [`Journal::open`] validates an existing binary journal, accepts and removes only an
/// incomplete final record (a torn append), rejects complete corruption, and resumes
/// appending at the last proven boundary. [`Journal::try_append`] writes and syncs a complete
/// length-delimited record before making the event visible in memory.
#[derive(Debug)]
pub struct Journal {
    events: Vec<JournalEvent>,
    storage: Option<DurableStorage>,
    max_bytes: usize,
    encoded_len: usize,
    last_error: Option<JournalError>,
    poisoned: bool,
}

impl Default for Journal {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            storage: None,
            max_bytes: DEFAULT_MAX_JOURNAL_BYTES,
            encoded_len: HEADER_LEN,
            last_error: None,
            poisoned: false,
        }
    }
}

impl Clone for Journal {
    fn clone(&self) -> Self {
        // A clone is intentionally detached from the file handle. Two appenders to one case
        // journal would violate the single-writer ordering contract.
        Self {
            events: self.events.clone(),
            storage: None,
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

    /// Create a new durable journal without overwriting any existing path.
    ///
    /// # Errors
    /// Returns [`JournalError::AlreadyExists`] for an existing target, or an I/O error.
    pub fn create_new(path: impl AsRef<Path>, max_bytes: usize) -> Result<Self, JournalError> {
        if max_bytes < HEADER_LEN {
            return Err(JournalError::LimitTooSmall);
        }
        let path = path.as_ref().to_path_buf();
        let mut file = match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                return Err(JournalError::AlreadyExists);
            }
            Err(error) => return Err(error.into()),
        };
        file.write_all(&header())?;
        file.sync_all()?;
        Ok(Self {
            events: Vec::new(),
            storage: Some(DurableStorage { path, file }),
            max_bytes,
            encoded_len: HEADER_LEN,
            last_error: None,
            poisoned: false,
        })
    }

    /// Open and validate a durable journal, creating it only when the path is absent.
    ///
    /// An incomplete final record is treated as a torn append and removed before the file
    /// is reopened for appending. A complete record with a bad checksum or payload is
    /// rejected; corruption is never silently skipped.
    ///
    /// # Errors
    /// Returns a typed format, bound, or I/O error.
    pub fn open(path: impl AsRef<Path>, max_bytes: usize) -> Result<Self, JournalError> {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            return Self::create_new(path, max_bytes);
        }
        if max_bytes < HEADER_LEN {
            return Err(JournalError::LimitTooSmall);
        }
        let mut file = OpenOptions::new().read(true).write(true).open(&path)?;
        let file_len = usize::try_from(file.metadata()?.len())
            .map_err(|_| JournalError::Full { limit: max_bytes })?;
        if file_len > max_bytes {
            return Err(JournalError::Full { limit: max_bytes });
        }
        let mut bytes = Vec::with_capacity(file_len);
        file.read_to_end(&mut bytes)?;
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
            let record_len = 4 + payload_len as usize + 4;
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

        if proven_len != bytes.len() {
            file.set_len(proven_len as u64)?;
            file.seek(SeekFrom::Start(proven_len as u64))?;
            file.sync_data()?;
        } else {
            file.seek(SeekFrom::End(0))?;
        }
        Ok(Self {
            events,
            storage: Some(DurableStorage { path, file }),
            max_bytes,
            encoded_len: proven_len,
            last_error: None,
            poisoned: false,
        })
    }

    /// Append and sync an event under the explicit byte bound.
    ///
    /// # Errors
    /// A typed bound, encoding or I/O error. The in-memory sequence advances only after a
    /// durable write completes.
    pub fn try_append(&mut self, event: JournalEvent) -> Result<(), JournalError> {
        if self.poisoned {
            let error = JournalError::Poisoned;
            self.last_error = Some(error.clone());
            return Err(error);
        }
        if matches!(
            &event,
            JournalEvent::TransientWriteApplied { field, .. }
                if *field != "beeper_off_flags"
        ) {
            let error = JournalError::InvalidRecord {
                offset: self.encoded_len,
            };
            self.last_error = Some(error.clone());
            return Err(error);
        }
        let payload = encode_event(&event);
        if payload.len() > MAX_RECORD_PAYLOAD_BYTES {
            let error = JournalError::RecordTooLarge(payload.len() as u32);
            self.last_error = Some(error.clone());
            return Err(error);
        }
        let next_len = self
            .encoded_len
            .checked_add(RECORD_OVERHEAD + payload.len())
            .ok_or(JournalError::Full {
                limit: self.max_bytes,
            })?;
        if next_len > self.max_bytes {
            let error = JournalError::Full {
                limit: self.max_bytes,
            };
            self.last_error = Some(error.clone());
            return Err(error);
        }
        let mut record = Vec::with_capacity(RECORD_OVERHEAD + payload.len());
        record.extend((payload.len() as u32).to_le_bytes());
        record.extend(&payload);
        record.extend(checksum(&payload).to_le_bytes());
        if let Some(storage) = &mut self.storage {
            if let Err(error) = storage
                .file
                .write_all(&record)
                .and_then(|_| storage.file.flush())
                .and_then(|_| storage.file.sync_data())
            {
                let error = JournalError::Io(error.kind());
                // `write_all`, `flush`, or `sync_data` may have advanced the file cursor or
                // persisted only a prefix. No later append may guess where the durable end
                // is; the caller must drop this handle and reopen through validation.
                self.poisoned = true;
                self.last_error = Some(error.clone());
                return Err(error);
            }
        }
        self.events.push(event);
        self.encoded_len = next_len;
        self.last_error = None;
        Ok(())
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

    /// The durable path, when this journal is file-backed.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.storage.as_ref().map(|storage| storage.path.as_path())
    }

    /// Bytes proven and retained under the format contract.
    #[must_use]
    pub const fn encoded_len(&self) -> usize {
        self.encoded_len
    }
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
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_path(label: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "ade-casebook-{label}-{}-{n}.journal",
            std::process::id()
        ))
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
        let path = temp_path("roundtrip");
        {
            let mut journal = Journal::create_new(&path, 1024).unwrap();
            journal.try_append(JournalEvent::IdentityRead).unwrap();
            journal
                .try_append(JournalEvent::WriteAhead {
                    class: WriteCommandClass::PersistentConfig,
                    recovery: RecoveryClass::AutomaticRollbackSupported,
                })
                .unwrap();
        }
        let bytes = fs::read(&path).unwrap();
        assert_eq!(
            &bytes[..17],
            &[
                b'A', b'D', b'E', b'J', 1, 0, 0, 0, // v1 header
                1, 0, 0, 0, 2, // IdentityRead record
                0x45, 0x60, 0x0c, 0x07, // FNV-1a checksum
            ]
        );
        let reopened = Journal::open(&path, 1024).unwrap();
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
        drop(reopened);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn torn_tail_is_removed_before_the_next_append() {
        let path = temp_path("torn-tail");
        let proven_len;
        {
            let mut journal = Journal::create_new(&path, 1024).unwrap();
            journal.try_append(JournalEvent::IdentityRead).unwrap();
            proven_len = journal.encoded_len();
        }
        {
            let mut file = OpenOptions::new().append(true).open(&path).unwrap();
            file.write_all(&[6, 0, 0, 0, 5, 1]).unwrap();
            file.sync_all().unwrap();
        }
        let mut reopened = Journal::open(&path, 1024).unwrap();
        assert_eq!(reopened.encoded_len(), proven_len);
        assert_eq!(fs::metadata(&path).unwrap().len(), proven_len as u64);
        reopened.try_append(JournalEvent::SnapshotRead).unwrap();
        drop(reopened);
        let final_read = Journal::open(&path, 1024).unwrap();
        assert_eq!(
            final_read.events(),
            &[JournalEvent::IdentityRead, JournalEvent::SnapshotRead]
        );
        drop(final_read);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn complete_checksum_corruption_is_rejected_not_truncated() {
        let path = temp_path("checksum");
        {
            let mut journal = Journal::create_new(&path, 1024).unwrap();
            journal.try_append(JournalEvent::IdentityRead).unwrap();
        }
        let original_len = fs::metadata(&path).unwrap().len();
        {
            let mut file = OpenOptions::new().write(true).open(&path).unwrap();
            file.seek(SeekFrom::Start((HEADER_LEN + 4) as u64)).unwrap();
            file.write_all(&[3]).unwrap();
            file.sync_all().unwrap();
        }
        assert_eq!(
            Journal::open(&path, 1024).unwrap_err(),
            JournalError::ChecksumMismatch { offset: HEADER_LEN }
        );
        assert_eq!(fs::metadata(&path).unwrap().len(), original_len);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn create_new_refuses_to_overwrite_an_existing_case() {
        let path = temp_path("no-overwrite");
        let journal = Journal::create_new(&path, 1024).unwrap();
        assert_eq!(
            Journal::create_new(&path, 1024).unwrap_err(),
            JournalError::AlreadyExists
        );
        drop(journal);
        fs::remove_file(path).unwrap();
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
        for (label, bytes, expected) in [
            (
                "magic",
                vec![b'B', b'A', b'D', b'!', 1, 0, 0, 0],
                JournalError::InvalidMagic,
            ),
            (
                "version",
                vec![b'A', b'D', b'E', b'J', 2, 0, 0, 0],
                JournalError::UnsupportedVersion(2),
            ),
            (
                "oversize",
                {
                    let mut bytes = header().to_vec();
                    bytes.extend(((MAX_RECORD_PAYLOAD_BYTES + 1) as u32).to_le_bytes());
                    bytes
                },
                JournalError::RecordTooLarge((MAX_RECORD_PAYLOAD_BYTES + 1) as u32),
            ),
        ] {
            let path = temp_path(label);
            fs::write(&path, bytes).unwrap();
            assert_eq!(Journal::open(&path, 1024).unwrap_err(), expected);
            fs::remove_file(path).unwrap();
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
    fn an_io_failed_handle_is_poisoned_until_reopened() {
        let path = temp_path("poisoned");
        let mut journal = Journal::create_new(&path, 1024).unwrap();
        // Replace the writable handle with a read-only one to deterministically force the
        // first durable append to fail without platform-specific permissions or low-level code.
        journal.storage.as_mut().unwrap().file = File::open(&path).unwrap();
        assert!(matches!(
            journal.try_append(JournalEvent::IdentityRead),
            Err(JournalError::Io(_))
        ));
        assert_eq!(
            journal.try_append(JournalEvent::SnapshotRead),
            Err(JournalError::Poisoned)
        );
        assert!(journal.events().is_empty());
        drop(journal);
        let reopened = Journal::open(&path, 1024).unwrap();
        assert!(reopened.events().is_empty());
        drop(reopened);
        fs::remove_file(path).unwrap();
    }
}
