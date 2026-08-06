#![forbid(unsafe_code)]

//! # `ade-casebook` — local case journal and resume reconciliation (M1)
//!
//! A local-only, append-only [`Journal`] of lifecycle events, from which the orchestrator
//! can be deterministically reconstructed after a process restart. The [`CaseRecord`] schema
//! is versioned and, by construction, carries **no** hardware-derived or tracking identifier:
//! the case id is supplied by the host and must not be derived from the device.

use ade_facts::DeviceIdentity;
use ade_safety::{ExecutionTarget, RecoveryClass, WriteCommandClass};

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

/// An append-only journal of lifecycle events (local only, never uploaded).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Journal {
    events: Vec<JournalEvent>,
}

impl Journal {
    /// A new, empty journal.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an event.
    pub fn append(&mut self, event: JournalEvent) {
        self.events.push(event);
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
}

/// Decide how to resume from a journal after a restart. This never assumes success and never
/// silently rolls back a transient write.
#[must_use]
pub fn reconcile_on_resume(journal: &Journal) -> ReconcileDecision {
    match journal.last() {
        None => ReconcileDecision::Continue,
        Some(event) => match event {
            JournalEvent::TransientWriteApplied { .. } => {
                ReconcileDecision::ReconcileTransientWrite
            }
            JournalEvent::Saved => ReconcileDecision::VerifySaveOutcome,
            JournalEvent::Verified | JournalEvent::Restored | JournalEvent::StateUnknown => {
                ReconcileDecision::AlreadyTerminal
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
}
