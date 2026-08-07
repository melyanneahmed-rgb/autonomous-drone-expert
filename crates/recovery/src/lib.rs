#![forbid(unsafe_code)]

//! # `ade-recovery` — restore-from-backup recovery for the M1 beeper slice
//!
//! Recovery runs entirely in [`ade_session::SessionState::Recovering`] and uses **only** the
//! pre-change [`Backup`] — never an assumed or reconstructed value. Its contract:
//!
//! * identity is re-proven before any recovery write; a different identity stops recovery
//!   immediately with **no** write;
//! * the current state is **read first**: if the pre-change value is already present (the
//!   failed write never applied), recovery verifies in place and performs no arbitrary
//!   write;
//! * otherwise the previous `beeper_off_flags` is restored via the exact four-byte SET
//!   (`RestoreFromBackupSupported`), re-read, saved (`RestoreFromBackupSupported`),
//!   rebooted (`ManualRecoveryRequired`), re-identified and finally re-read;
//! * every unprovable outcome is an honest [`RecoveryResult::StateUnknown`] — never a
//!   claimed rollback.

use ade_backup::Backup;
use ade_casebook::{Journal, JournalEvent};
use ade_execution::{Executor, WriteOperation};
use ade_facts::IdentityMatch;
use ade_protocol_msp::BeeperConfigSnapshot;
use ade_safety::{
    ExecutionTarget, RecoveryClass, WriteApproval, WriteCommandClass, WriteGateError,
    authorize_write,
};
use ade_session::SessionState;
use ade_transport::{LogicalTransport, PhasedTransport};

/// The approvals a recovery needs, obtained through the safety gate for a simulation target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryApprovals {
    /// Approval for the restore SET (`TransientConfig` / `RestoreFromBackupSupported`).
    pub restore_set: WriteApproval,
    /// Approval for the restore save (`PersistentConfig` / `RestoreFromBackupSupported`).
    pub restore_save: WriteApproval,
    /// Approval for the recovery reboot (`Reboot` / `ManualRecoveryRequired`).
    pub reboot: WriteApproval,
}

impl RecoveryApprovals {
    /// Obtain the three recovery approvals for a **simulation** target.
    ///
    /// # Errors
    /// [`WriteGateError::HardwareGateNotApproved`] for the hardware target (the gate is
    /// checked first inside [`authorize_write`]).
    pub fn obtain(target: ExecutionTarget) -> Result<Self, WriteGateError> {
        Ok(Self {
            restore_set: authorize_write(
                target,
                WriteCommandClass::TransientConfig,
                RecoveryClass::RestoreFromBackupSupported,
            )?,
            restore_save: authorize_write(
                target,
                WriteCommandClass::PersistentConfig,
                RecoveryClass::RestoreFromBackupSupported,
            )?,
            reboot: authorize_write(
                target,
                WriteCommandClass::Reboot,
                RecoveryClass::ManualRecoveryRequired,
            )?,
        })
    }
}

/// The stage at which a recovery failed, for the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStage {
    /// Durable journal evidence could not be written before/after a recovery action.
    Journal,
    /// Re-identification before any recovery write.
    ReIdentify,
    /// The re-identified device is not the one the backup belongs to.
    IdentityMismatch,
    /// Reading the current state before deciding.
    ReadCurrent,
    /// The restore SET.
    RestoreSet,
    /// The re-read between the restore SET and the save.
    ReReadBeforeSave,
    /// The DShot fields no longer match the backup.
    DshotMismatch,
    /// The restore save.
    Save,
    /// The recovery reboot.
    Reboot,
    /// Reconnecting after the recovery reboot.
    Reconnect,
    /// Re-identification after the recovery reboot.
    PostRebootIdentify,
    /// The post-reboot identity differs.
    PostRebootIdentityMismatch,
    /// The final read.
    FinalRead,
    /// The final value does not equal the backup value.
    FinalVerify,
}

/// Evidence collected by a recovery run (metadata and typed values only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryEvidence {
    /// Whether identity was re-proven before any write.
    pub identity_reproven: bool,
    /// The value recovery restores (from the backup).
    pub restore_off_flags: u32,
    /// The snapshot read before deciding, if it was reached.
    pub pre_restore_snapshot: Option<BeeperConfigSnapshot>,
    /// Whether the previous value was already present, making a restore write unnecessary.
    pub verified_in_place: bool,
    /// The final snapshot, if it was read.
    pub final_snapshot: Option<BeeperConfigSnapshot>,
    /// The stage that failed, if any.
    pub failed_stage: Option<RecoveryStage>,
}

/// The outcome of a recovery run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryResult {
    /// The previous value was verified restored (or verified still present).
    Restored(RecoveryEvidence),
    /// The state could not be proven. Never claimed as a rollback.
    StateUnknown(RecoveryEvidence),
}

impl RecoveryResult {
    /// The evidence, whichever way the recovery ended.
    #[must_use]
    pub fn evidence(&self) -> &RecoveryEvidence {
        match self {
            RecoveryResult::Restored(e) | RecoveryResult::StateUnknown(e) => e,
        }
    }
}

fn dshot_matches(snapshot: &BeeperConfigSnapshot, backup: &Backup) -> bool {
    snapshot.dshot_beacon_tone == backup.beeper_snapshot.dshot_beacon_tone
        && snapshot.dshot_beacon_off_flags == backup.beeper_snapshot.dshot_beacon_off_flags
}

/// Run the restore-from-backup recovery. The caller must have transitioned the session to
/// [`SessionState::Recovering`]; all I/O here runs under that state's authority. On success
/// the journal gains [`JournalEvent::Restored`]; on any unprovable path it gains
/// [`JournalEvent::StateUnknown`].
pub fn run_restore<T: LogicalTransport + PhasedTransport>(
    executor: &mut Executor,
    transport: &mut T,
    backup: &Backup,
    journal: &mut Journal,
    approvals: &RecoveryApprovals,
) -> RecoveryResult {
    let state = SessionState::Recovering;
    let mut evidence = RecoveryEvidence {
        identity_reproven: false,
        restore_off_flags: backup.restore_value(),
        pre_restore_snapshot: None,
        verified_in_place: false,
        final_snapshot: None,
        failed_stage: None,
    };
    let fail = |mut e: RecoveryEvidence, stage, journal: &mut Journal| {
        e.failed_stage = Some(stage);
        let _ = journal.try_append(JournalEvent::StateUnknown);
        RecoveryResult::StateUnknown(e)
    };
    if journal.try_append(JournalEvent::RecoveryStarted).is_err() {
        return fail(evidence, RecoveryStage::Journal, journal);
    }

    // 1. Re-prove identity before anything else. No write happens before this point.
    transport.begin_identification();
    let identity = match executor.identify(transport, state) {
        Ok(identity) => identity,
        Err(_) => return fail(evidence, RecoveryStage::ReIdentify, journal),
    };
    if identity.compare(&backup.identity) != IdentityMatch::Same {
        // A different device: stop immediately, write nothing.
        return fail(evidence, RecoveryStage::IdentityMismatch, journal);
    }
    evidence.identity_reproven = true;

    // 2. Read the current state first; never write blindly.
    transport.enter_operational();
    let current = match executor.read_snapshot(transport, state) {
        Ok(snapshot) => snapshot,
        Err(_) => return fail(evidence, RecoveryStage::ReadCurrent, journal),
    };
    evidence.pre_restore_snapshot = Some(current.clone());

    // 3. If the pre-change value is already present (the failed write never applied) the
    //    state is proven by reading alone: no arbitrary recovery write.
    if current.beeper_off_flags == backup.restore_value() && dshot_matches(&current, backup) {
        evidence.verified_in_place = true;
        evidence.final_snapshot = Some(current);
        if journal.try_append(JournalEvent::Restored).is_err() {
            return fail(evidence, RecoveryStage::Journal, journal);
        }
        return RecoveryResult::Restored(evidence);
    }

    // 4. Restore the previous value with the exact four-byte SET.
    if journal
        .try_append(JournalEvent::WriteAhead {
            class: WriteCommandClass::TransientConfig,
            recovery: RecoveryClass::RestoreFromBackupSupported,
        })
        .is_err()
    {
        return fail(evidence, RecoveryStage::Journal, journal);
    }
    if executor
        .write(
            transport,
            state,
            WriteOperation::SetBeeperOffFlags(backup.restore_value()),
            &approvals.restore_set,
            RecoveryClass::RestoreFromBackupSupported,
        )
        .is_err()
    {
        return fail(evidence, RecoveryStage::RestoreSet, journal);
    }
    if journal
        .try_append(JournalEvent::TransientWriteApplied {
            field: "beeper_off_flags",
            mask: backup.desired_delta.mask,
        })
        .is_err()
    {
        return fail(evidence, RecoveryStage::Journal, journal);
    }

    // 5. Re-read before the recovery save.
    let reread = match executor.read_snapshot(transport, state) {
        Ok(snapshot) => snapshot,
        Err(_) => return fail(evidence, RecoveryStage::ReReadBeforeSave, journal),
    };
    if reread.beeper_off_flags != backup.restore_value() {
        return fail(evidence, RecoveryStage::ReReadBeforeSave, journal);
    }
    if !dshot_matches(&reread, backup) {
        return fail(evidence, RecoveryStage::DshotMismatch, journal);
    }
    if journal.try_append(JournalEvent::ReReadBeforeSave).is_err() {
        return fail(evidence, RecoveryStage::Journal, journal);
    }

    // 6. Save, 7. reboot — both under their declared recovery classes.
    if journal
        .try_append(JournalEvent::WriteAhead {
            class: WriteCommandClass::PersistentConfig,
            recovery: RecoveryClass::RestoreFromBackupSupported,
        })
        .is_err()
    {
        return fail(evidence, RecoveryStage::Journal, journal);
    }
    if executor
        .write(
            transport,
            state,
            WriteOperation::SaveEeprom,
            &approvals.restore_save,
            RecoveryClass::RestoreFromBackupSupported,
        )
        .is_err()
    {
        return fail(evidence, RecoveryStage::Save, journal);
    }
    if journal.try_append(JournalEvent::Saved).is_err() {
        return fail(evidence, RecoveryStage::Journal, journal);
    }
    if journal
        .try_append(JournalEvent::WriteAhead {
            class: WriteCommandClass::Reboot,
            recovery: RecoveryClass::ManualRecoveryRequired,
        })
        .is_err()
    {
        return fail(evidence, RecoveryStage::Journal, journal);
    }
    if executor
        .write(
            transport,
            state,
            WriteOperation::Reboot,
            &approvals.reboot,
            RecoveryClass::ManualRecoveryRequired,
        )
        .is_err()
    {
        return fail(evidence, RecoveryStage::Reboot, journal);
    }
    if journal.try_append(JournalEvent::Rebooted).is_err() {
        return fail(evidence, RecoveryStage::Journal, journal);
    }

    // 8. Reconnect and re-prove identity again.
    transport.close();
    if transport.open().is_err() {
        return fail(evidence, RecoveryStage::Reconnect, journal);
    }
    transport.begin_identification();
    let after = match executor.identify(transport, state) {
        Ok(identity) => identity,
        Err(_) => return fail(evidence, RecoveryStage::PostRebootIdentify, journal),
    };
    if after.compare(&backup.identity) != IdentityMatch::Same {
        return fail(evidence, RecoveryStage::PostRebootIdentityMismatch, journal);
    }

    // 9. Final read: the previous mask must be restored exactly, DShot untouched.
    transport.enter_operational();
    let final_snapshot = match executor.read_snapshot(transport, state) {
        Ok(snapshot) => snapshot,
        Err(_) => return fail(evidence, RecoveryStage::FinalRead, journal),
    };
    evidence.final_snapshot = Some(final_snapshot.clone());
    if final_snapshot.beeper_off_flags != backup.restore_value()
        || !dshot_matches(&final_snapshot, backup)
    {
        return fail(evidence, RecoveryStage::FinalVerify, journal);
    }
    if journal.try_append(JournalEvent::Restored).is_err() {
        return fail(evidence, RecoveryStage::Journal, journal);
    }
    RecoveryResult::Restored(evidence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_approvals_are_refused_for_the_hardware_target() {
        assert!(matches!(
            RecoveryApprovals::obtain(ExecutionTarget::Hardware),
            Err(WriteGateError::HardwareGateNotApproved(_)),
        ));
        assert!(RecoveryApprovals::obtain(ExecutionTarget::Mock).is_ok());
        assert!(RecoveryApprovals::obtain(ExecutionTarget::Replay).is_ok());
    }
}
