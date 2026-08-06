#![forbid(unsafe_code)]

//! # `ade-core-api` — the M1 beeper lifecycle orchestrator (Mock/Replay only)
//!
//! The public surface receives an execution target, a typed goal, a simulation transport,
//! host-supplied case metadata and simulation approval evidence — and returns a structured
//! [`M1RunReport`]. It never accepts or exposes raw MSP bytes, raw command ids or arbitrary
//! payloads: every outbound command flows through the single [`ade_execution::Executor`]
//! path, which enforces the session command-authority matrix, the write-approval evidence
//! and the fixed payload shapes before any exchange.
//!
//! The hardware target is refused before any transport is opened. Every terminal path —
//! verified, restored, no-op, scope mismatch, aborted, state-unknown — produces a report
//! carrying the explicit markers `NO HARDWARE CONTACTED`, `NO HARDWARE SUPPORT CLAIM` and
//! `REQUIRES HARDWARE TEST`.

use ade_backup::{Backup, DesiredDelta};
use ade_capability::m1_proposed_target;
use ade_casebook::{
    CaseRecord, Journal, JournalEvent, ReconcileDecision, RecoveryOutcome, VerificationOutcome,
    reconcile_on_resume,
};
use ade_execution::{ExecError, Executor, WriteOperation};
use ade_facts::{DeviceIdentity, IdentityMatch};
use ade_planning::{BeeperPlan, SystemInitBeeperGoal, VerificationRequirements, build_plan};
use ade_protocol_msp::{BeeperConfigSnapshot, CommandId, SYSTEM_INIT_OFF_MASK};
use ade_recovery::{RecoveryApprovals, RecoveryEvidence, RecoveryResult, run_restore};
use ade_safety::{
    ExecutionTarget, RecoveryClass, WriteApproval, WriteCommandClass, WriteGateError,
    authorize_write,
};
use ade_session::{Session, SessionState};
use ade_transport::{AuditAccess, AuditDisposition, AuditEntry, LogicalTransport, PhasedTransport};

/// The explicit markers carried by every report.
pub const REPORT_MARKERS: [&str; 3] = [
    "NO HARDWARE CONTACTED",
    "NO HARDWARE SUPPORT CLAIM",
    "REQUIRES HARDWARE TEST",
];

/// Host-supplied case metadata. Never derived from the device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseMetadata {
    /// Host-supplied case identifier (never hardware-derived).
    pub case_id: String,
    /// Host-supplied start label (no wall clock is read here).
    pub started_at_label: String,
}

/// The approval evidence a simulation run carries: one [`WriteApproval`] per write class of
/// the happy path, plus the recovery approvals. Obtainable only through the safety gate, so
/// hardware evidence cannot exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationApprovals {
    /// `TransientConfig` / `TransientWritePendingReconcileOnResume`.
    pub transient: WriteApproval,
    /// `PersistentConfig` / `AutomaticRollbackSupported`.
    pub persistent: WriteApproval,
    /// `Reboot` / `ManualRecoveryRequired`.
    pub reboot: WriteApproval,
    /// The recovery-path approvals (`RestoreFromBackupSupported` writes).
    pub recovery: RecoveryApprovals,
}

impl SimulationApprovals {
    /// Obtain the full approval evidence for a **simulation** target.
    ///
    /// # Errors
    /// [`WriteGateError::HardwareGateNotApproved`] for the hardware target.
    pub fn obtain(target: ExecutionTarget) -> Result<Self, WriteGateError> {
        Ok(Self {
            transient: authorize_write(
                target,
                WriteCommandClass::TransientConfig,
                RecoveryClass::TransientWritePendingReconcileOnResume,
            )?,
            persistent: authorize_write(
                target,
                WriteCommandClass::PersistentConfig,
                RecoveryClass::AutomaticRollbackSupported,
            )?,
            reboot: authorize_write(
                target,
                WriteCommandClass::Reboot,
                RecoveryClass::ManualRecoveryRequired,
            )?,
            recovery: RecoveryApprovals::obtain(target)?,
        })
    }
}

/// Whether the identified device is inside the pinned M1 scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeStatus {
    /// Not yet checked (identification did not complete).
    NotChecked,
    /// The identity matches the pinned scope. The board itself remains
    /// `PROPOSED — NOT HARDWARE VALIDATED`.
    InScope,
    /// The identity is outside the pinned scope; no write lifecycle is started.
    Mismatch {
        /// The first field that failed the scope check.
        field: &'static str,
    },
}

/// The lifecycle stage at which a run failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureStage {
    /// Opening the logical transport.
    Open,
    /// Initial identification.
    Identify,
    /// The scope check after identification.
    ScopeCheck,
    /// Reading the initial snapshot.
    SnapshotRead,
    /// Plan construction/validation.
    Planning,
    /// The transient SET write.
    TransientWrite,
    /// The re-read between the SET and the save.
    ReReadBeforeSave,
    /// The verification of the re-read before the save.
    VerifyBeforeSave,
    /// The EEPROM save.
    Save,
    /// The reboot request.
    Reboot,
    /// Re-opening the connection after the reboot.
    Reconnect,
    /// Re-identification after the reboot.
    PostRebootIdentify,
    /// The device that returned is not the device the session started with.
    IdentityMismatchAfterReboot,
    /// The final verification read.
    VerifyRead,
    /// The final verification comparison.
    VerifyMismatch,
    /// A resume found a value that is neither the previous nor the desired one.
    ResumeUnexpectedValue,
    /// A resume re-identification failed or found a different device.
    ResumeIdentity,
    /// A rebuilt report found the journal's evidence chain incomplete.
    ResumeIncompleteEvidence,
}

/// The terminal classification of a run. Every value maps to a fixed readiness string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalClassification {
    /// The intended change was applied, saved, survived a reboot and was verified.
    CompletedVerified,
    /// The previous value was restored (or verified still present) and verified.
    CompletedRestored,
    /// The state could not be proven.
    StateUnknownRecoveryRequired,
    /// The desired state was already present; nothing was sent, reads verified it.
    NoOpVerified,
    /// The device is outside the pinned scope; nothing was written.
    ScopeMismatch,
    /// The run aborted before any write reached the device.
    AbortedBeforeAnyWrite,
    /// The hardware target was refused before any transport was opened.
    HardwareRefused,
}

impl TerminalClassification {
    /// The readiness classification string for this terminal.
    #[must_use]
    pub const fn readiness(self) -> &'static str {
        match self {
            TerminalClassification::CompletedVerified => {
                "VERIFIED ON SIMULATION — NOT HARDWARE READY"
            }
            TerminalClassification::CompletedRestored => {
                "RESTORED ON SIMULATION — NOT HARDWARE READY"
            }
            TerminalClassification::StateUnknownRecoveryRequired => {
                "STATE UNKNOWN — RECOVERY REQUIRED"
            }
            TerminalClassification::NoOpVerified => {
                "NO-OP VERIFIED ON SIMULATION — DESIRED STATE ALREADY PRESENT"
            }
            TerminalClassification::ScopeMismatch => "NOT READY — SCOPE MISMATCH",
            TerminalClassification::AbortedBeforeAnyWrite => "NOT READY — ABORTED BEFORE ANY WRITE",
            TerminalClassification::HardwareRefused => "HARDWARE_WRITE_GATE_NOT_APPROVED",
        }
    }
}

/// The verification evidence of a run (typed values only; never raw payloads).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerificationEvidence {
    /// The exact `beeper_off_flags` the verification expected.
    pub expected_off_flags: u32,
    /// The `beeper_off_flags` that was observed.
    pub observed_off_flags: u32,
    /// The DShot tone required and observed.
    pub required_dshot_tone: u8,
    /// The observed DShot tone.
    pub observed_dshot_tone: u8,
    /// The DShot off-flags required.
    pub required_dshot_off_flags: u32,
    /// The observed DShot off-flags.
    pub observed_dshot_off_flags: u32,
    /// Whether every requirement held.
    pub matched: bool,
}

/// A compact summary of the plan for the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanSummary {
    /// `beeper_off_flags` before.
    pub previous_off_flags: u32,
    /// `beeper_off_flags` intended.
    pub desired_off_flags: u32,
    /// The exact changed bits.
    pub changed_bits: u32,
    /// The SYSTEM_INIT mask.
    pub system_init_mask: u32,
    /// Whether the plan was an explicit no-op.
    pub is_noop: bool,
}

/// The structured report of one M1 run. Contains no signature, no raw MSP frame, no raw
/// payload, no serial/USB UID and no GPS/home/network identifier — none are representable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M1RunReport {
    /// The simulation target.
    pub execution_target: ExecutionTarget,
    /// The scope status after identification.
    pub scope: ScopeStatus,
    /// The initial composite identity (signature is not part of it).
    pub initial_identity: Option<DeviceIdentity>,
    /// The typed goal.
    pub goal: SystemInitBeeperGoal,
    /// The initial snapshot.
    pub initial_snapshot: Option<BeeperConfigSnapshot>,
    /// The plan summary.
    pub plan_summary: Option<PlanSummary>,
    /// The classes of the outbound commands actually sent, in order (audit-derived).
    pub command_classes: Vec<WriteCommandClass>,
    /// The recovery classes declared by the plan's write steps (plus the recovery path when
    /// a recovery ran).
    pub recovery_classes: Vec<RecoveryClass>,
    /// The full ordered session-state history.
    pub session_history: Vec<SessionState>,
    /// The journal checkpoints, in order (event names).
    pub checkpoints: Vec<String>,
    /// The outbound frame audit (metadata only, including blocked attempts).
    pub audit: Vec<AuditEntry>,
    /// The verification outcome.
    pub verification: VerificationOutcome,
    /// The verification evidence, when a verification read happened.
    pub verification_evidence: Option<VerificationEvidence>,
    /// The stage that failed, if any.
    pub failure_stage: Option<FailureStage>,
    /// Whether a recovery ran.
    pub recovery_attempted: bool,
    /// The recovery evidence, when a recovery ran.
    pub recovery_evidence: Option<RecoveryEvidence>,
    /// The recovery outcome.
    pub recovery_outcome: RecoveryOutcome,
    /// The terminal classification.
    pub terminal: TerminalClassification,
    /// The session state the run ended in.
    pub terminal_session_state: SessionState,
    /// The readiness classification string.
    pub readiness: &'static str,
    /// `MOCK_EXERCISED` or `REPLAY_EXERCISED` — never `HARDWARE_OBSERVED`.
    pub verification_state: &'static str,
    /// The explicit non-hardware markers.
    pub markers: [&'static str; 3],
    /// The versioned case record.
    pub case: CaseRecord,
}

/// The exercise label for a simulation target. `HARDWARE_OBSERVED` is unreachable: the
/// hardware target never runs, so its label is a refusal marker, not an observation claim.
#[must_use]
pub const fn verification_state_label(target: ExecutionTarget) -> &'static str {
    match target {
        ExecutionTarget::Mock => "MOCK_EXERCISED",
        ExecutionTarget::Replay => "REPLAY_EXERCISED",
        ExecutionTarget::Hardware => "NOT_EXERCISED_HARDWARE_REFUSED",
    }
}

/// Check the identified device against the pinned M1 scope
/// (`PROPOSED — NOT HARDWARE VALIDATED`; a scope match is not a support claim).
#[must_use]
pub fn check_scope(identity: &DeviceIdentity) -> ScopeStatus {
    let proposed = m1_proposed_target();
    if identity.api.protocol_version != 0 {
        return ScopeStatus::Mismatch {
            field: "protocol_version",
        };
    }
    if identity.api.api_major != 1 || identity.api.api_minor != 46 {
        return ScopeStatus::Mismatch {
            field: "msp_api_version",
        };
    }
    if &identity.variant.identifier != b"BTFL" {
        return ScopeStatus::Mismatch {
            field: "fc_variant",
        };
    }
    if (
        identity.version.major,
        identity.version.minor,
        identity.version.patch,
    ) != (4, 5, 5)
    {
        return ScopeStatus::Mismatch {
            field: "fc_version",
        };
    }
    if identity.target_name != proposed.betaflight_target {
        return ScopeStatus::Mismatch {
            field: "target_name",
        };
    }
    ScopeStatus::InScope
}

fn command_class_of(command: u8) -> WriteCommandClass {
    match CommandId::from_u8(command) {
        Some(CommandId::SetBeeperConfig) => WriteCommandClass::TransientConfig,
        Some(CommandId::EepromWrite) => WriteCommandClass::PersistentConfig,
        Some(CommandId::Reboot) => WriteCommandClass::Reboot,
        _ => WriteCommandClass::NoWrite,
    }
}

fn check_verification(
    requirements: &VerificationRequirements,
    observed: &BeeperConfigSnapshot,
) -> VerificationEvidence {
    let matched = observed.beeper_off_flags == requirements.expected_off_flags
        && observed.dshot_beacon_tone == requirements.required_dshot_tone
        && observed.dshot_beacon_off_flags == requirements.required_dshot_off_flags
        && (observed.beeper_off_flags ^ requirements.expected_off_flags)
            & requirements.unchanged_bits_mask
            == 0;
    VerificationEvidence {
        expected_off_flags: requirements.expected_off_flags,
        observed_off_flags: observed.beeper_off_flags,
        required_dshot_tone: requirements.required_dshot_tone,
        observed_dshot_tone: observed.dshot_beacon_tone,
        required_dshot_off_flags: requirements.required_dshot_off_flags,
        observed_dshot_off_flags: observed.dshot_beacon_off_flags,
        matched,
    }
}

/// Derive the goal a backup was created for (from its stored delta).
#[must_use]
pub const fn goal_from_backup(backup: &Backup) -> SystemInitBeeperGoal {
    if backup.desired_delta.next & SYSTEM_INIT_OFF_MASK != 0 {
        SystemInitBeeperGoal::Disable
    } else {
        SystemInitBeeperGoal::Enable
    }
}

fn contains_subsequence(events: &[JournalEvent], required: &[JournalEvent]) -> bool {
    let mut iter = events.iter();
    required
        .iter()
        .all(|r| iter.any(|e| core::mem::discriminant(e) == core::mem::discriminant(r)))
}

struct Engine<T> {
    target: ExecutionTarget,
    goal: SystemInitBeeperGoal,
    transport: T,
    executor: Executor,
    session: Session,
    journal: Journal,
    approvals: SimulationApprovals,
    case_meta: CaseMetadata,
    scope: ScopeStatus,
    identity: Option<DeviceIdentity>,
    initial_snapshot: Option<BeeperConfigSnapshot>,
    plan: Option<BeeperPlan>,
    backup: Option<Backup>,
    failure_stage: Option<FailureStage>,
    recovery_attempted: bool,
    recovery_evidence: Option<RecoveryEvidence>,
    verification_evidence: Option<VerificationEvidence>,
}

impl<T: LogicalTransport + PhasedTransport + AuditAccess> Engine<T> {
    fn new(
        target: ExecutionTarget,
        goal: SystemInitBeeperGoal,
        transport: T,
        executor: Executor,
        case_meta: CaseMetadata,
        approvals: SimulationApprovals,
        journal: Journal,
    ) -> Self {
        Self {
            target,
            goal,
            transport,
            executor,
            session: Session::new(),
            journal,
            approvals,
            case_meta,
            scope: ScopeStatus::NotChecked,
            identity: None,
            initial_snapshot: None,
            plan: None,
            backup: None,
            failure_stage: None,
            recovery_attempted: false,
            recovery_evidence: None,
            verification_evidence: None,
        }
    }

    fn tr(&mut self, next: SessionState) {
        // Every transition in this engine follows the validated machine; a refusal would be
        // an internal ordering bug and must not panic mid-lifecycle, so it is ignored and
        // the honest terminal classification still governs the report.
        let _ = self.session.transition_to(next);
    }

    fn identify_now(&mut self) -> Result<DeviceIdentity, ExecError> {
        let state = self.session.state();
        self.executor.identify(&mut self.transport, state)
    }

    fn read_snapshot_now(&mut self) -> Result<BeeperConfigSnapshot, ExecError> {
        let state = self.session.state();
        self.executor.read_snapshot(&mut self.transport, state)
    }

    fn write_now(
        &mut self,
        operation: WriteOperation,
        approval: &WriteApproval,
        declared: RecoveryClass,
    ) -> Result<(), ExecError> {
        let state = self.session.state();
        self.executor
            .write(&mut self.transport, state, operation, approval, declared)
    }

    /// Abort before any write reached the device: close, disconnect, classify honestly.
    fn abort_before_write(&mut self, stage: FailureStage) -> TerminalClassification {
        self.failure_stage = Some(stage);
        self.transport.close();
        self.tr(SessionState::Disconnected);
        TerminalClassification::AbortedBeforeAnyWrite
    }

    /// Enter recovery after a failure that may have changed the device.
    fn recover(&mut self, stage: FailureStage) -> TerminalClassification {
        self.failure_stage = Some(stage);
        self.tr(SessionState::Recovering);
        let Some(backup) = self.backup.clone() else {
            // No backup means no write was ever authorised; this path is unreachable after
            // a write, and without evidence the only honest terminal is state-unknown.
            self.journal.append(JournalEvent::StateUnknown);
            self.tr(SessionState::StateUnknownRecoveryRequired);
            return TerminalClassification::StateUnknownRecoveryRequired;
        };
        let result = run_restore(
            &mut self.executor,
            &mut self.transport,
            &backup,
            &mut self.journal,
            &self.approvals.recovery,
        );
        self.recovery_attempted = true;
        self.recovery_evidence = Some(result.evidence().clone());
        match result {
            RecoveryResult::Restored(_) => {
                self.tr(SessionState::CompletedRestored);
                TerminalClassification::CompletedRestored
            }
            RecoveryResult::StateUnknown(_) => {
                self.tr(SessionState::StateUnknownRecoveryRequired);
                TerminalClassification::StateUnknownRecoveryRequired
            }
        }
    }

    /// Connect and run initial identification; on success the identity is stored.
    fn connect_and_identify(
        &mut self,
        must_match: Option<&DeviceIdentity>,
    ) -> Result<(), TerminalClassification> {
        self.tr(SessionState::Connecting);
        if self.transport.open().is_err() {
            return Err(self.abort_before_write(FailureStage::Open));
        }
        self.tr(SessionState::Identifying);
        let identity = match self.identify_now() {
            Ok(identity) => identity,
            Err(_) => return Err(self.abort_before_write(FailureStage::Identify)),
        };
        if let Some(expected) = must_match {
            if identity.compare(expected) != IdentityMatch::Same {
                self.failure_stage = Some(FailureStage::ResumeIdentity);
                self.journal.append(JournalEvent::StateUnknown);
                self.tr(SessionState::StateUnknownRecoveryRequired);
                return Err(TerminalClassification::StateUnknownRecoveryRequired);
            }
        }
        self.journal.append(JournalEvent::IdentityRead);
        self.identity = Some(identity);
        Ok(())
    }

    /// The full fresh run.
    fn drive(&mut self) -> TerminalClassification {
        self.journal.append(JournalEvent::Started {
            execution_target: self.target,
        });
        if let Err(terminal) = self.connect_and_identify(None) {
            return terminal;
        }

        // Scope gate: outside the pinned scope nothing further happens — no backup, no
        // write, no readiness.
        let identity = self.identity.clone().expect("identity read above");
        self.scope = check_scope(&identity);
        if let ScopeStatus::Mismatch { .. } = self.scope {
            self.failure_stage = Some(FailureStage::ScopeCheck);
            self.transport.close();
            self.tr(SessionState::Disconnected);
            return TerminalClassification::ScopeMismatch;
        }

        // Snapshot.
        self.tr(SessionState::SnapshotRead);
        self.transport.enter_operational();
        let snapshot = match self.read_snapshot_now() {
            Ok(snapshot) => snapshot,
            Err(_) => return self.abort_before_write(FailureStage::SnapshotRead),
        };
        self.journal.append(JournalEvent::SnapshotRead);
        self.initial_snapshot = Some(snapshot.clone());

        // Plan.
        let Ok(plan) = build_plan(&snapshot, self.goal) else {
            return self.abort_before_write(FailureStage::Planning);
        };
        self.plan = Some(plan.clone());

        if plan.is_noop {
            // Explicit no-op: verification reads only — no SET, no EEPROM write, no reboot.
            let verify = match self.read_snapshot_now() {
                Ok(snapshot) => snapshot,
                Err(_) => return self.abort_before_write(FailureStage::VerifyRead),
            };
            let evidence = check_verification(&plan.verification, &verify);
            self.verification_evidence = Some(evidence);
            if !evidence.matched {
                return self.abort_before_write(FailureStage::VerifyMismatch);
            }
            self.journal.append(JournalEvent::Verified);
            self.tr(SessionState::Planning);
            return TerminalClassification::NoOpVerified;
        }

        self.tr(SessionState::Planning);
        if plan.validate().is_err() {
            return self.abort_before_write(FailureStage::Planning);
        }
        self.tr(SessionState::AwaitingApproval);
        // The approval evidence is carried as values; the executor re-verifies each one
        // against the concrete write before any frame is sent.
        self.tr(SessionState::BackingUp);
        let mut backup = Backup::new(
            identity,
            snapshot.clone(),
            DesiredDelta {
                field: "beeper_off_flags",
                mask: plan.system_init_mask,
                previous: plan.previous_off_flags,
                next: plan.desired_off_flags,
            },
            RecoveryClass::RestoreFromBackupSupported,
        );
        backup.provenance_refs = plan
            .provenance_records
            .iter()
            .map(|r| (*r).to_string())
            .collect();
        backup
            .checkpoints
            .push("backed-up-before-any-write".to_string());
        self.backup = Some(backup);
        self.journal.append(JournalEvent::BackedUp);

        self.tr(SessionState::ApplyingTransient);
        self.apply_and_finish(true)
    }

    /// From `ApplyingTransient`: optionally send the SET, then re-read, save, reboot and
    /// verify. `send_set` is false when a resume proved the transient value is already
    /// present on the device.
    fn apply_and_finish(&mut self, send_set: bool) -> TerminalClassification {
        let plan = self.plan.clone().expect("plan built before applying");
        if send_set {
            let approval = self.approvals.transient.clone();
            if self
                .write_now(
                    WriteOperation::SetBeeperOffFlags(plan.desired_off_flags),
                    &approval,
                    RecoveryClass::TransientWritePendingReconcileOnResume,
                )
                .is_err()
            {
                return self.recover(FailureStage::TransientWrite);
            }
            self.journal.append(JournalEvent::TransientWriteApplied {
                field: "beeper_off_flags",
                mask: plan.system_init_mask,
            });
        }

        // Re-read before the save, still in ApplyingTransient.
        let reread = match self.read_snapshot_now() {
            Ok(snapshot) => snapshot,
            Err(_) => return self.recover(FailureStage::ReReadBeforeSave),
        };
        let evidence = check_verification(&plan.verification, &reread);
        if !evidence.matched {
            return self.recover(FailureStage::VerifyBeforeSave);
        }
        self.journal.append(JournalEvent::ReReadBeforeSave);

        self.tr(SessionState::Saving);
        let approval = self.approvals.persistent.clone();
        if self
            .write_now(
                WriteOperation::SaveEeprom,
                &approval,
                RecoveryClass::AutomaticRollbackSupported,
            )
            .is_err()
        {
            return self.recover(FailureStage::Save);
        }
        self.journal.append(JournalEvent::Saved);

        self.reboot_and_verify()
    }

    /// From `Saving`: reboot, reconnect, re-identify (same identity required), re-read and
    /// verify the intended bit only.
    fn reboot_and_verify(&mut self) -> TerminalClassification {
        let plan = self.plan.clone().expect("plan built before rebooting");
        self.tr(SessionState::Rebooting);
        let approval = self.approvals.reboot.clone();
        if self
            .write_now(
                WriteOperation::Reboot,
                &approval,
                RecoveryClass::ManualRecoveryRequired,
            )
            .is_err()
        {
            return self.recover(FailureStage::Reboot);
        }
        self.journal.append(JournalEvent::Rebooted);

        self.tr(SessionState::Reconnecting);
        self.transport.close();
        if self.transport.open().is_err() {
            return self.recover(FailureStage::Reconnect);
        }
        self.journal.append(JournalEvent::Reconnected);

        self.tr(SessionState::Verifying);
        self.transport.begin_identification();
        let after = match self.identify_now() {
            Ok(identity) => identity,
            Err(_) => return self.recover(FailureStage::PostRebootIdentify),
        };
        let initial = self.identity.clone().expect("initial identity present");
        if after.compare(&initial) != IdentityMatch::Same {
            // A different device came back: stop immediately. No recovery write may be
            // aimed at a different identity.
            self.failure_stage = Some(FailureStage::IdentityMismatchAfterReboot);
            self.journal.append(JournalEvent::StateUnknown);
            self.tr(SessionState::StateUnknownRecoveryRequired);
            return TerminalClassification::StateUnknownRecoveryRequired;
        }

        self.transport.enter_operational();
        let final_snapshot = match self.read_snapshot_now() {
            Ok(snapshot) => snapshot,
            Err(_) => return self.recover(FailureStage::VerifyRead),
        };
        let evidence = check_verification(&plan.verification, &final_snapshot);
        self.verification_evidence = Some(evidence);
        if !evidence.matched {
            return self.recover(FailureStage::VerifyMismatch);
        }
        self.journal.append(JournalEvent::Verified);
        self.tr(SessionState::CompletedVerified);
        TerminalClassification::CompletedVerified
    }

    /// Resume after a restart with a transient write potentially in flight
    /// (`TRANSIENT_WRITE_PENDING — RECONCILE_ON_RESUME`).
    fn resume_reconcile(&mut self, backup: &Backup) -> TerminalClassification {
        if let Err(terminal) = self.connect_and_identify(Some(&backup.identity)) {
            return terminal;
        }
        self.scope = check_scope(&backup.identity);
        self.tr(SessionState::SnapshotRead);
        self.transport.enter_operational();
        let current = match self.read_snapshot_now() {
            Ok(snapshot) => snapshot,
            Err(_) => return self.abort_before_write(FailureStage::SnapshotRead),
        };
        let previous = backup.desired_delta.previous;
        let desired = backup.desired_delta.next;
        let dshot_ok = current.dshot_beacon_tone == backup.beeper_snapshot.dshot_beacon_tone
            && current.dshot_beacon_off_flags == backup.beeper_snapshot.dshot_beacon_off_flags;

        let send_set = if current.beeper_off_flags == desired && dshot_ok {
            // The transient write survived: continue with re-read-before-save then save.
            false
        } else if current.beeper_off_flags == previous && dshot_ok {
            // The transient write was lost: re-apply it under fresh approval evidence.
            true
        } else {
            // A third value: nothing about this state is provable from the journal.
            self.failure_stage = Some(FailureStage::ResumeUnexpectedValue);
            self.journal.append(JournalEvent::StateUnknown);
            self.tr(SessionState::StateUnknownRecoveryRequired);
            return TerminalClassification::StateUnknownRecoveryRequired;
        };

        self.tr(SessionState::Planning);
        self.tr(SessionState::AwaitingApproval);
        self.tr(SessionState::BackingUp);
        self.tr(SessionState::ApplyingTransient);
        self.apply_and_finish(send_set)
    }

    /// Resume after a restart with a save in flight: never assume the save's outcome —
    /// reboot, reconnect, re-identify and verify from reads.
    fn resume_verify_save(&mut self, backup: &Backup) -> TerminalClassification {
        if let Err(terminal) = self.connect_and_identify(Some(&backup.identity)) {
            return terminal;
        }
        self.scope = check_scope(&backup.identity);
        self.tr(SessionState::SnapshotRead);
        self.transport.enter_operational();
        // The current RAM value is evidence but proves nothing about the EEPROM; the
        // verdict comes from the post-reboot read.
        if self.read_snapshot_now().is_err() {
            return self.abort_before_write(FailureStage::SnapshotRead);
        }
        self.tr(SessionState::Planning);
        self.tr(SessionState::AwaitingApproval);
        self.tr(SessionState::BackingUp);
        self.tr(SessionState::ApplyingTransient);
        self.tr(SessionState::Saving);
        self.reboot_and_verify()
    }

    fn into_report(self, terminal: TerminalClassification) -> M1RunReport {
        let audit = self.transport.audit_entries().to_vec();
        let command_classes = audit
            .iter()
            .filter(|entry| entry.disposition == AuditDisposition::Sent)
            .map(|entry| command_class_of(entry.command))
            .collect();
        let mut recovery_classes: Vec<RecoveryClass> = self
            .plan
            .as_ref()
            .map(|plan| plan.write_steps().iter().map(|s| s.recovery).collect())
            .unwrap_or_default();
        if self.recovery_attempted {
            if let Some(plan) = self.plan.as_ref() {
                recovery_classes.push(plan.recovery_path.set_recovery);
                recovery_classes.push(plan.recovery_path.save_recovery);
                recovery_classes.push(plan.recovery_path.reboot_recovery);
            }
        }
        let verification = match terminal {
            TerminalClassification::CompletedVerified | TerminalClassification::NoOpVerified => {
                VerificationOutcome::IntendedBitOnly
            }
            _ => match self.verification_evidence {
                Some(evidence) if !evidence.matched => VerificationOutcome::Failed,
                _ => VerificationOutcome::NotPerformed,
            },
        };
        let recovery_outcome = match terminal {
            TerminalClassification::CompletedRestored => RecoveryOutcome::Restored,
            TerminalClassification::StateUnknownRecoveryRequired => RecoveryOutcome::StateUnknown,
            _ => RecoveryOutcome::NotRequired,
        };
        let mut case = CaseRecord::start(
            self.case_meta.case_id.clone(),
            self.case_meta.started_at_label.clone(),
            self.target,
        );
        case.initial_identity = self.identity.clone();
        case.outbound_classes = audit
            .iter()
            .filter(|entry| entry.disposition == AuditDisposition::Sent)
            .map(|entry| command_class_of(entry.command))
            .collect();
        case.recovery_class = self
            .backup
            .as_ref()
            .map_or(RecoveryClass::NotApplicableNoWrite, |b| b.recovery_class);
        case.verification = verification;
        case.recovery = recovery_outcome;
        case.terminal_state = Some(format!("{terminal:?}"));
        M1RunReport {
            execution_target: self.target,
            scope: self.scope,
            initial_identity: self.identity,
            goal: self.goal,
            initial_snapshot: self.initial_snapshot,
            plan_summary: self.plan.as_ref().map(|plan| PlanSummary {
                previous_off_flags: plan.previous_off_flags,
                desired_off_flags: plan.desired_off_flags,
                changed_bits: plan.changed_bits,
                system_init_mask: plan.system_init_mask,
                is_noop: plan.is_noop,
            }),
            command_classes,
            recovery_classes,
            session_history: self.session.history().to_vec(),
            checkpoints: self
                .journal
                .events()
                .iter()
                .map(|event| format!("{event:?}"))
                .collect(),
            audit,
            verification,
            verification_evidence: self.verification_evidence,
            failure_stage: self.failure_stage,
            recovery_attempted: self.recovery_attempted,
            recovery_evidence: self.recovery_evidence,
            recovery_outcome,
            terminal,
            terminal_session_state: self.session.state(),
            readiness: terminal.readiness(),
            verification_state: verification_state_label(self.target),
            markers: REPORT_MARKERS,
            case,
        }
    }
}

/// Run the full M1 beeper lifecycle on a simulation transport.
///
/// The hardware target is refused before the transport is opened: the report carries
/// [`TerminalClassification::HardwareRefused`] and no frame is ever produced.
pub fn run_beeper_lifecycle<T: LogicalTransport + PhasedTransport + AuditAccess>(
    target: ExecutionTarget,
    goal: SystemInitBeeperGoal,
    transport: T,
    case_meta: CaseMetadata,
    approvals: SimulationApprovals,
) -> M1RunReport {
    match Executor::new_simulation(target) {
        Ok(executor) => {
            let mut engine = Engine::new(
                target,
                goal,
                transport,
                executor,
                case_meta,
                approvals,
                Journal::new(),
            );
            let terminal = engine.drive();
            engine.into_report(terminal)
        }
        Err(_) => hardware_refused_report(target, goal, transport, &case_meta),
    }
}

/// Resume a previously interrupted run from its journal and pre-change backup.
///
/// Deterministic per the reconcile contract: a pending transient write is re-read and
/// reconciled (never silently rolled back); an in-flight save is verified through a reboot;
/// a journal that already reached a terminal state is rebuilt idempotently with **no** new
/// events and no transport I/O.
pub fn resume_beeper_lifecycle<T: LogicalTransport + PhasedTransport + AuditAccess>(
    target: ExecutionTarget,
    transport: T,
    case_meta: CaseMetadata,
    approvals: SimulationApprovals,
    journal: Journal,
    backup: Backup,
) -> M1RunReport {
    let goal = goal_from_backup(&backup);
    let executor = match Executor::new_simulation(target) {
        Ok(executor) => executor,
        Err(_) => return hardware_refused_report(target, goal, transport, &case_meta),
    };
    let decision = reconcile_on_resume(&journal);
    let mut engine = Engine::new(
        target, goal, transport, executor, case_meta, approvals, journal,
    );
    engine.plan = build_plan(&backup.beeper_snapshot, goal).ok();
    engine.initial_snapshot = Some(backup.beeper_snapshot.clone());
    engine.backup = Some(backup.clone());
    match decision {
        ReconcileDecision::AlreadyTerminal => rebuild_report(engine, &backup),
        ReconcileDecision::ReconcileTransientWrite | ReconcileDecision::Continue => {
            let terminal = engine.resume_reconcile(&backup);
            engine.into_report(terminal)
        }
        ReconcileDecision::VerifySaveOutcome => {
            let terminal = engine.resume_verify_save(&backup);
            engine.into_report(terminal)
        }
    }
}

/// Rebuild the report from an already-terminal journal: idempotent, no I/O, no new events,
/// and never a completed terminal without its full evidence chain.
fn rebuild_report<T: LogicalTransport + PhasedTransport + AuditAccess>(
    mut engine: Engine<T>,
    backup: &Backup,
) -> M1RunReport {
    engine.identity = Some(backup.identity.clone());
    engine.scope = check_scope(&backup.identity);
    let events = engine.journal.events().to_vec();
    let wrote = events
        .iter()
        .any(|e| matches!(e, JournalEvent::TransientWriteApplied { .. }));
    let terminal = match events.last() {
        Some(JournalEvent::Verified) if wrote => {
            let chain = [
                JournalEvent::Started {
                    execution_target: engine.target,
                },
                JournalEvent::IdentityRead,
                JournalEvent::SnapshotRead,
                JournalEvent::BackedUp,
                JournalEvent::TransientWriteApplied {
                    field: "beeper_off_flags",
                    mask: SYSTEM_INIT_OFF_MASK,
                },
                JournalEvent::ReReadBeforeSave,
                JournalEvent::Saved,
                JournalEvent::Rebooted,
                JournalEvent::Reconnected,
                JournalEvent::Verified,
            ];
            if contains_subsequence(&events, &chain) {
                TerminalClassification::CompletedVerified
            } else {
                engine.failure_stage = Some(FailureStage::ResumeIncompleteEvidence);
                TerminalClassification::StateUnknownRecoveryRequired
            }
        }
        Some(JournalEvent::Verified) => {
            let chain = [
                JournalEvent::Started {
                    execution_target: engine.target,
                },
                JournalEvent::IdentityRead,
                JournalEvent::SnapshotRead,
                JournalEvent::Verified,
            ];
            if contains_subsequence(&events, &chain) {
                TerminalClassification::NoOpVerified
            } else {
                engine.failure_stage = Some(FailureStage::ResumeIncompleteEvidence);
                TerminalClassification::StateUnknownRecoveryRequired
            }
        }
        Some(JournalEvent::Restored) => {
            let chain = [
                JournalEvent::Started {
                    execution_target: engine.target,
                },
                JournalEvent::IdentityRead,
                JournalEvent::RecoveryStarted,
                JournalEvent::Restored,
            ];
            if contains_subsequence(&events, &chain) {
                engine.recovery_attempted = true;
                TerminalClassification::CompletedRestored
            } else {
                engine.failure_stage = Some(FailureStage::ResumeIncompleteEvidence);
                TerminalClassification::StateUnknownRecoveryRequired
            }
        }
        _ => TerminalClassification::StateUnknownRecoveryRequired,
    };
    engine.into_report(terminal)
}

fn hardware_refused_report<T: LogicalTransport + PhasedTransport + AuditAccess>(
    target: ExecutionTarget,
    goal: SystemInitBeeperGoal,
    transport: T,
    case_meta: &CaseMetadata,
) -> M1RunReport {
    // The transport is deliberately never opened and never exchanged with.
    let mut case = CaseRecord::start(
        case_meta.case_id.clone(),
        case_meta.started_at_label.clone(),
        target,
    );
    case.terminal_state = Some(format!("{:?}", TerminalClassification::HardwareRefused));
    M1RunReport {
        execution_target: target,
        scope: ScopeStatus::NotChecked,
        initial_identity: None,
        goal,
        initial_snapshot: None,
        plan_summary: None,
        command_classes: Vec::new(),
        recovery_classes: Vec::new(),
        session_history: vec![SessionState::Disconnected],
        checkpoints: Vec::new(),
        audit: transport.audit_entries().to_vec(),
        verification: VerificationOutcome::NotPerformed,
        verification_evidence: None,
        failure_stage: None,
        recovery_attempted: false,
        recovery_evidence: None,
        recovery_outcome: RecoveryOutcome::NotRequired,
        terminal: TerminalClassification::HardwareRefused,
        terminal_session_state: SessionState::Disconnected,
        readiness: TerminalClassification::HardwareRefused.readiness(),
        verification_state: verification_state_label(target),
        markers: REPORT_MARKERS,
        case,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ade_fault_injection::{Fault, FaultInjector, ScheduledFaultInjector};
    use ade_mock_fc::MockFc;
    use ade_protocol_msp::{Direction, SetBeeperConfig, decode_frame, encode_frame};
    use ade_transport::{
        FrameResponder, InMemoryAudit, MockTransport, ReplayResponse, ReplayStep, ReplayTransport,
        TransportError,
    };
    use std::cell::Cell;
    use std::rc::Rc;

    const PREV: u32 = 0;
    const DESIRED: u32 = SYSTEM_INIT_OFF_MASK;
    const TONE: u8 = 3;
    const DSHOT_FLAGS: u32 = 0x0000_0011;

    // Exchange ordinals of the happy path (0-based).
    const ORD_IDENTIFY_FIRST: usize = 0;
    const ORD_SNAPSHOT: usize = 4;
    const ORD_SET: usize = 5;
    const ORD_REREAD: usize = 6;
    const ORD_SAVE: usize = 7;
    const ORD_REBOOT: usize = 8;
    // Recovery ordinals after a failure at ORD_SAVE.
    const ORD_RECOVERY_SET: usize = 13;
    const ORD_RECOVERY_SAVE: usize = 15;
    const ORD_RECOVERY_REBOOT: usize = 16;
    const ORD_RECOVERY_FINAL_READ: usize = 21;

    fn snapshot(off_flags: u32) -> BeeperConfigSnapshot {
        BeeperConfigSnapshot {
            beeper_off_flags: off_flags,
            dshot_beacon_tone: TONE,
            dshot_beacon_off_flags: DSHOT_FLAGS,
        }
    }

    fn meta() -> CaseMetadata {
        CaseMetadata {
            case_id: "case-m1-0001".to_string(),
            started_at_label: "t0".to_string(),
        }
    }

    fn approvals() -> SimulationApprovals {
        SimulationApprovals::obtain(ExecutionTarget::Mock).expect("simulation approvals")
    }

    fn run_mock<R: FrameResponder>(responder: R) -> M1RunReport {
        run_beeper_lifecycle(
            ExecutionTarget::Mock,
            SystemInitBeeperGoal::Disable,
            MockTransport::new(responder, InMemoryAudit::new()),
            meta(),
            approvals(),
        )
    }

    fn run_with_faults(schedule: Vec<(usize, Fault)>, initial: u32) -> M1RunReport {
        run_mock(ScheduledFaultInjector::new(
            MockFc::new(snapshot(initial)),
            schedule,
        ))
    }

    fn mock_identity() -> DeviceIdentity {
        DeviceIdentity {
            api: ade_protocol_msp::ApiVersion {
                protocol_version: 0,
                api_major: 1,
                api_minor: 46,
            },
            variant: ade_protocol_msp::FcVariant {
                identifier: *b"BTFL",
            },
            version: ade_protocol_msp::FcVersion {
                major: 4,
                minor: 5,
                patch: 5,
            },
            board_identifier: *b"S405",
            hardware_revision: 0,
            fc_type: 0,
            target_capabilities: 0,
            target_name: "SPEEDYBEEF405V4".to_string(),
            board_name: "SpeedyBee F405 V4".to_string(),
            manufacturer_id: "SPB".to_string(),
            mcu_type_id: 0,
        }
    }

    fn backup_for_change() -> Backup {
        let mut backup = Backup::new(
            mock_identity(),
            snapshot(PREV),
            DesiredDelta {
                field: "beeper_off_flags",
                mask: SYSTEM_INIT_OFF_MASK,
                previous: PREV,
                next: DESIRED,
            },
            RecoveryClass::RestoreFromBackupSupported,
        );
        backup.provenance_refs = vec!["bf-4.5.5-beeper-system-init-bit".to_string()];
        backup
    }

    fn set_frame(flags: u32) -> Vec<u8> {
        SetBeeperConfig::new(flags).encode_request().unwrap()
    }

    fn sent_write_commands(report: &M1RunReport) -> Vec<u8> {
        report
            .audit
            .iter()
            .filter(|e| e.disposition == AuditDisposition::Sent)
            .map(|e| e.command)
            .filter(|c| CommandId::from_u8(*c).is_some_and(ade_transport::is_write_command))
            .collect()
    }

    fn checkpoint_count(report: &M1RunReport, name: &str) -> usize {
        report
            .checkpoints
            .iter()
            .filter(|c| c.contains(name))
            .count()
    }

    // ---- responder wrappers (test-only; production code has no such API) ----

    /// ACKs `MSP_EEPROM_WRITE` without delegating: the save silently never commits.
    struct SaveDropper {
        fc: MockFc,
    }
    impl FrameResponder for SaveDropper {
        fn respond(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
            let frame = decode_frame(request).map_err(TransportError::Malformed)?;
            if frame.known_command() == Some(CommandId::EepromWrite) {
                return encode_frame(Direction::Reply, CommandId::EepromWrite, &[])
                    .map_err(TransportError::Malformed);
            }
            self.fc.respond(request)
        }
    }

    /// Delegates normally until the reboot, then swaps the board identifier: a different
    /// device comes back after the reboot.
    struct IdentitySwapAfterReboot {
        fc: MockFc,
    }
    impl FrameResponder for IdentitySwapAfterReboot {
        fn respond(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
            let frame = decode_frame(request).map_err(TransportError::Malformed)?;
            let reply = self.fc.respond(request)?;
            if frame.known_command() == Some(CommandId::Reboot) {
                self.fc.set_board_identifier(*b"XXXX");
            }
            Ok(reply)
        }
    }

    /// Delegates normally until the reboot; afterwards the device never answers again.
    struct DeadAfterReboot {
        fc: MockFc,
        dead: bool,
    }
    impl FrameResponder for DeadAfterReboot {
        fn respond(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
            if self.dead {
                return Err(TransportError::Disconnected);
            }
            let frame = decode_frame(request).map_err(TransportError::Malformed)?;
            let reply = self.fc.respond(request)?;
            if frame.known_command() == Some(CommandId::Reboot) {
                self.dead = true;
            }
            Ok(reply)
        }
    }

    /// Rewrites the firmware version reply to 4.5.6: an out-of-scope device.
    struct VersionRewriter {
        fc: MockFc,
    }
    impl FrameResponder for VersionRewriter {
        fn respond(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
            let frame = decode_frame(request).map_err(TransportError::Malformed)?;
            if frame.known_command() == Some(CommandId::FcVersion) {
                return encode_frame(Direction::Reply, CommandId::FcVersion, &[4, 5, 6])
                    .map_err(TransportError::Malformed);
            }
            self.fc.respond(request)
        }
    }

    /// Repeats the previous reply verbatim at one chosen ordinal: a duplicate reply.
    struct RepeatsReplyAt {
        fc: MockFc,
        at: usize,
        calls: usize,
        last: Option<Vec<u8>>,
    }
    impl FrameResponder for RepeatsReplyAt {
        fn respond(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
            let ordinal = self.calls;
            self.calls += 1;
            if ordinal == self.at {
                if let Some(last) = self.last.clone() {
                    return Ok(last);
                }
            }
            let reply = self.fc.respond(request)?;
            self.last = Some(reply.clone());
            Ok(reply)
        }
    }

    /// Simulated power loss during the EEPROM write: the device resets (RAM reloads from
    /// EEPROM) and the connection drops; the save never commits.
    struct PowerLossAtSave {
        fc: MockFc,
        fired: bool,
    }
    impl FrameResponder for PowerLossAtSave {
        fn respond(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
            let frame = decode_frame(request).map_err(TransportError::Malformed)?;
            if frame.known_command() == Some(CommandId::EepromWrite) && !self.fired {
                self.fired = true;
                // Model the power cycle with the mock's own reboot semantics (test-only).
                let reboot = encode_frame(Direction::Request, CommandId::Reboot, &[]).unwrap();
                let _ = self.fc.respond(&reboot)?;
                return Err(TransportError::Disconnected);
            }
            self.fc.respond(request)
        }
    }

    /// After `n` of its own delegated calls, the board identifier changes (used to make the
    /// recovery re-identification meet a different device).
    struct SwapIdentityAfterCalls {
        fc: MockFc,
        n: usize,
        calls: usize,
    }
    impl FrameResponder for SwapIdentityAfterCalls {
        fn respond(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
            if self.calls >= self.n {
                self.fc.set_board_identifier(*b"XXXX");
            }
            self.calls += 1;
            self.fc.respond(request)
        }
    }

    /// Answers the very first request with a reply for a command that was never requested.
    struct UnsolicitedFirstReply {
        fc: MockFc,
        calls: usize,
    }
    impl FrameResponder for UnsolicitedFirstReply {
        fn respond(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
            let ordinal = self.calls;
            self.calls += 1;
            if ordinal == 0 {
                return encode_frame(Direction::Reply, CommandId::EepromWrite, &[])
                    .map_err(TransportError::Malformed);
            }
            self.fc.respond(request)
        }
    }

    /// A transport that records whether it was ever opened or exchanged with.
    struct ProbeNeverUsed {
        opened: Rc<Cell<bool>>,
        exchanged: Rc<Cell<bool>>,
    }
    impl LogicalTransport for ProbeNeverUsed {
        fn open(&mut self) -> Result<(), TransportError> {
            self.opened.set(true);
            Ok(())
        }
        fn exchange(&mut self, _request: &[u8]) -> Result<Vec<u8>, TransportError> {
            self.exchanged.set(true);
            Err(TransportError::NotOpen)
        }
        fn close(&mut self) {}
    }
    impl PhasedTransport for ProbeNeverUsed {
        fn enter_operational(&mut self) {}
        fn begin_identification(&mut self) {}
    }
    impl AuditAccess for ProbeNeverUsed {
        fn audit_entries(&self) -> &[AuditEntry] {
            &[]
        }
    }

    fn happy_requests() -> Vec<Vec<u8>> {
        let read = |cmd| encode_frame(Direction::Request, cmd, &[]).unwrap();
        vec![
            read(CommandId::ApiVersion),
            read(CommandId::FcVariant),
            read(CommandId::FcVersion),
            read(CommandId::BoardInfo),
            read(CommandId::BeeperConfig),
            set_frame(DESIRED),
            read(CommandId::BeeperConfig),
            read(CommandId::EepromWrite),
            read(CommandId::Reboot),
            read(CommandId::ApiVersion),
            read(CommandId::FcVariant),
            read(CommandId::FcVersion),
            read(CommandId::BoardInfo),
            read(CommandId::BeeperConfig),
        ]
    }

    fn happy_transcript() -> Vec<ReplayStep> {
        let mut fc = MockFc::new(snapshot(PREV));
        happy_requests()
            .into_iter()
            .map(|request| {
                let reply = fc.respond(&request).expect("mock replies");
                ReplayStep {
                    expected_request: request,
                    response: ReplayResponse::Reply(reply),
                }
            })
            .collect()
    }

    const FULL_WALK: [SessionState; 13] = [
        SessionState::Disconnected,
        SessionState::Connecting,
        SessionState::Identifying,
        SessionState::SnapshotRead,
        SessionState::Planning,
        SessionState::AwaitingApproval,
        SessionState::BackingUp,
        SessionState::ApplyingTransient,
        SessionState::Saving,
        SessionState::Rebooting,
        SessionState::Reconnecting,
        SessionState::Verifying,
        SessionState::CompletedVerified,
    ];

    // ---- happy paths ----

    #[test]
    fn the_happy_path_completes_verified_on_mock() {
        let report = run_mock(MockFc::new(snapshot(PREV)));
        assert_eq!(report.terminal, TerminalClassification::CompletedVerified);
        assert_eq!(
            report.terminal_session_state,
            SessionState::CompletedVerified
        );
        assert_eq!(report.session_history, FULL_WALK.to_vec());
        assert_eq!(report.scope, ScopeStatus::InScope);
        assert_eq!(report.verification, VerificationOutcome::IntendedBitOnly);
        assert_eq!(report.verification_state, "MOCK_EXERCISED");
        assert_eq!(report.markers, REPORT_MARKERS);
        assert!(!report.recovery_attempted);

        // Journal chain, in order.
        for (index, name) in [
            "Started",
            "IdentityRead",
            "SnapshotRead",
            "BackedUp",
            "TransientWriteApplied",
            "ReReadBeforeSave",
            "Saved",
            "Rebooted",
            "Reconnected",
            "Verified",
        ]
        .iter()
        .enumerate()
        {
            assert!(
                report.checkpoints[index].contains(name),
                "checkpoint {index} should be {name}, got {}",
                report.checkpoints[index],
            );
        }

        // Audit: 14 sent frames, none blocked; identify reads empty; SET exactly 4 bytes;
        // the re-read precedes the save.
        assert_eq!(report.audit.len(), 14);
        assert!(
            report
                .audit
                .iter()
                .all(|e| e.disposition == AuditDisposition::Sent)
        );
        for entry in &report.audit[ORD_IDENTIFY_FIRST..=3] {
            assert_eq!(entry.payload_len, 0, "identification reads are empty");
        }
        assert_eq!(
            report.audit[ORD_SET].command,
            CommandId::SetBeeperConfig.as_u8()
        );
        assert_eq!(report.audit[ORD_SET].payload_len, 4);
        assert_eq!(
            report.audit[ORD_REREAD].command,
            CommandId::BeeperConfig.as_u8()
        );
        assert_eq!(
            report.audit[ORD_SAVE].command,
            CommandId::EepromWrite.as_u8()
        );

        // The backup exists before the first transient send.
        let backed_up = report
            .checkpoints
            .iter()
            .position(|c| c.contains("BackedUp"))
            .unwrap();
        let transient = report
            .checkpoints
            .iter()
            .position(|c| c.contains("TransientWriteApplied"))
            .unwrap();
        assert!(backed_up < transient);

        // Verification evidence: intended bit only, DShot untouched.
        let evidence = report.verification_evidence.unwrap();
        assert!(evidence.matched);
        assert_eq!(evidence.observed_off_flags, DESIRED);
        assert_eq!(evidence.observed_dshot_tone, TONE);
        assert_eq!(evidence.observed_dshot_off_flags, DSHOT_FLAGS);

        // Plan summary.
        let plan = report.plan_summary.unwrap();
        assert_eq!(
            plan.previous_off_flags ^ plan.desired_off_flags,
            plan.system_init_mask
        );
        assert!(!plan.is_noop);

        // Command classes as sent.
        assert_eq!(
            report.command_classes[ORD_SET],
            WriteCommandClass::TransientConfig
        );
        assert_eq!(
            report.command_classes[ORD_SAVE],
            WriteCommandClass::PersistentConfig
        );
        assert_eq!(
            report.command_classes[ORD_REBOOT],
            WriteCommandClass::Reboot
        );

        // Nothing in the report ever claims hardware observation, and no signature or raw
        // payload can appear in its Debug rendering.
        let debug = format!("{report:?}");
        assert!(!debug.contains("HARDWARE_OBSERVED"));
        assert!(!debug.contains("signature"));
        assert!(report.readiness.contains("NOT HARDWARE READY"));
    }

    #[test]
    fn the_happy_path_completes_verified_on_replay() {
        let transport = ReplayTransport::new(happy_transcript(), InMemoryAudit::new());
        let report = run_beeper_lifecycle(
            ExecutionTarget::Replay,
            SystemInitBeeperGoal::Disable,
            transport,
            meta(),
            SimulationApprovals::obtain(ExecutionTarget::Replay).unwrap(),
        );
        assert_eq!(report.terminal, TerminalClassification::CompletedVerified);
        assert_eq!(report.verification_state, "REPLAY_EXERCISED");
        assert_eq!(report.session_history, FULL_WALK.to_vec());
        assert_eq!(report.audit.len(), 14);
    }

    #[test]
    fn a_noop_plan_sends_no_set_no_eeprom_no_reboot() {
        // The bit is already set; the goal is Disable: an explicit no-op.
        let report = run_mock(MockFc::new(snapshot(DESIRED)));
        assert_eq!(report.terminal, TerminalClassification::NoOpVerified);
        assert!(report.plan_summary.unwrap().is_noop);
        assert!(sent_write_commands(&report).is_empty(), "no write was sent");
        // Only reads: four identification reads plus two snapshot reads.
        assert_eq!(report.audit.len(), 6);
        assert_eq!(checkpoint_count(&report, "Verified"), 1);
        assert!(report.verification_evidence.unwrap().matched);
        assert!(
            !report
                .session_history
                .contains(&SessionState::ApplyingTransient)
        );
    }

    // ---- gates ----

    #[test]
    fn the_hardware_target_is_refused_before_the_transport_opens() {
        let opened = Rc::new(Cell::new(false));
        let exchanged = Rc::new(Cell::new(false));
        let probe = ProbeNeverUsed {
            opened: Rc::clone(&opened),
            exchanged: Rc::clone(&exchanged),
        };
        let report = run_beeper_lifecycle(
            ExecutionTarget::Hardware,
            SystemInitBeeperGoal::Disable,
            probe,
            meta(),
            approvals(),
        );
        assert_eq!(report.terminal, TerminalClassification::HardwareRefused);
        assert_eq!(report.readiness, "HARDWARE_WRITE_GATE_NOT_APPROVED");
        assert!(!opened.get(), "the transport must never be opened");
        assert!(!exchanged.get(), "no frame may ever be produced");
        assert!(report.audit.is_empty());
        assert_eq!(report.verification_state, "NOT_EXERCISED_HARDWARE_REFUSED");
    }

    #[test]
    fn an_out_of_scope_device_gets_no_backup_and_no_write() {
        let report = run_mock(VersionRewriter {
            fc: MockFc::new(snapshot(PREV)),
        });
        assert_eq!(report.terminal, TerminalClassification::ScopeMismatch);
        assert_eq!(
            report.scope,
            ScopeStatus::Mismatch {
                field: "fc_version"
            }
        );
        assert_eq!(report.failure_stage, Some(FailureStage::ScopeCheck));
        assert!(sent_write_commands(&report).is_empty());
        assert_eq!(checkpoint_count(&report, "BackedUp"), 0);
        assert!(report.readiness.contains("NOT READY"));
        // The proposed board never became more than proposed.
        assert_eq!(
            m1_proposed_target().status.as_str(),
            "PROPOSED — NOT HARDWARE VALIDATED",
        );
    }

    // ---- the 26 mandatory failure scenarios ----

    /// 1. Mask mismatch after reboot (the save silently never committed).
    #[test]
    fn scenario_01_mask_mismatch_after_reboot_recovers_to_restored() {
        let report = run_mock(SaveDropper {
            fc: MockFc::new(snapshot(PREV)),
        });
        assert_eq!(report.terminal, TerminalClassification::CompletedRestored);
        assert_eq!(report.failure_stage, Some(FailureStage::VerifyMismatch));
        assert!(report.recovery_attempted);
        let recovery = report.recovery_evidence.unwrap();
        // After the reboot the device already shows the previous value, so recovery proves
        // the state by reading and performs no arbitrary write.
        assert!(recovery.verified_in_place);
        assert_eq!(report.recovery_outcome, RecoveryOutcome::Restored);
    }

    /// 2. EEPROM write timeout.
    #[test]
    fn scenario_02_eeprom_timeout_recovers_to_restored() {
        let report = run_with_faults(vec![(ORD_SAVE, Fault::Timeout)], PREV);
        assert_eq!(report.terminal, TerminalClassification::CompletedRestored);
        assert_eq!(report.failure_stage, Some(FailureStage::Save));
        let recovery = report.recovery_evidence.unwrap();
        assert!(!recovery.verified_in_place, "a real restore write ran");
        assert_eq!(recovery.final_snapshot.unwrap().beeper_off_flags, PREV);
    }

    /// 3. EEPROM write with no reply.
    #[test]
    fn scenario_03_eeprom_no_reply_recovers_to_restored() {
        let report = run_with_faults(vec![(ORD_SAVE, Fault::NoReply)], PREV);
        assert_eq!(report.terminal, TerminalClassification::CompletedRestored);
        assert_eq!(report.failure_stage, Some(FailureStage::Save));
    }

    /// 4. A corrupt frame in place of the SET reply: the write never reached the device —
    ///    recovery proves that by reading before writing anything.
    #[test]
    fn scenario_04_corrupt_frame_at_set_reads_before_any_recovery_write() {
        let report = run_with_faults(vec![(ORD_SET, Fault::CorruptFrame)], PREV);
        assert_eq!(report.terminal, TerminalClassification::CompletedRestored);
        assert_eq!(report.failure_stage, Some(FailureStage::TransientWrite));
        assert!(report.recovery_evidence.unwrap().verified_in_place);
    }

    /// 5. A bad checksum on the re-read before the save.
    #[test]
    fn scenario_05_bad_checksum_at_reread_recovers_to_restored() {
        let report = run_with_faults(vec![(ORD_REREAD, Fault::BadChecksum)], PREV);
        assert_eq!(report.terminal, TerminalClassification::CompletedRestored);
        assert_eq!(report.failure_stage, Some(FailureStage::ReReadBeforeSave));
        assert!(!report.recovery_evidence.unwrap().verified_in_place);
    }

    /// 6. A duplicate reply during identification never advances the lifecycle.
    #[test]
    fn scenario_06_duplicate_reply_aborts_before_any_write() {
        let report = run_mock(RepeatsReplyAt {
            fc: MockFc::new(snapshot(PREV)),
            at: 1,
            calls: 0,
            last: None,
        });
        assert_eq!(
            report.terminal,
            TerminalClassification::AbortedBeforeAnyWrite
        );
        assert_eq!(report.failure_stage, Some(FailureStage::Identify));
        assert!(sent_write_commands(&report).is_empty());
    }

    /// 7. An out-of-order transcript on Replay never advances the lifecycle.
    #[test]
    fn scenario_07_out_of_order_replay_aborts_before_any_write() {
        let mut steps = happy_transcript();
        steps.swap(0, 1);
        let transport = ReplayTransport::new(steps, InMemoryAudit::new());
        let report = run_beeper_lifecycle(
            ExecutionTarget::Replay,
            SystemInitBeeperGoal::Disable,
            transport,
            meta(),
            SimulationApprovals::obtain(ExecutionTarget::Replay).unwrap(),
        );
        assert_eq!(
            report.terminal,
            TerminalClassification::AbortedBeforeAnyWrite
        );
        assert!(sent_write_commands(&report).is_empty());
    }

    /// 8. The device never returns after the reboot.
    #[test]
    fn scenario_08_device_never_returns_after_reboot_is_state_unknown() {
        let report = run_mock(DeadAfterReboot {
            fc: MockFc::new(snapshot(PREV)),
            dead: false,
        });
        assert_eq!(
            report.terminal,
            TerminalClassification::StateUnknownRecoveryRequired
        );
        assert!(report.recovery_attempted);
        assert_eq!(
            report.recovery_evidence.unwrap().failed_stage,
            Some(ade_recovery::RecoveryStage::ReIdentify),
        );
        assert_eq!(report.recovery_outcome, RecoveryOutcome::StateUnknown);
    }

    /// 9. A different identity after the reboot stops everything, with no recovery write.
    #[test]
    fn scenario_09_different_identity_after_reboot_stops_without_recovery_write() {
        let report = run_mock(IdentitySwapAfterReboot {
            fc: MockFc::new(snapshot(PREV)),
        });
        assert_eq!(
            report.terminal,
            TerminalClassification::StateUnknownRecoveryRequired
        );
        assert_eq!(
            report.failure_stage,
            Some(FailureStage::IdentityMismatchAfterReboot)
        );
        assert!(
            !report.recovery_attempted,
            "no recovery may target a different device"
        );
        // Nothing was sent after the reboot except identification/verification reads.
        let post_reboot_writes: Vec<u8> = report.audit[ORD_REBOOT + 1..]
            .iter()
            .filter(|e| e.disposition == AuditDisposition::Sent)
            .map(|e| e.command)
            .filter(|c| CommandId::from_u8(*c).is_some_and(ade_transport::is_write_command))
            .collect();
        assert!(post_reboot_writes.is_empty());
        assert_eq!(report.readiness, "STATE UNKNOWN — RECOVERY REQUIRED");
    }

    /// 10. Restart between the transient write and the save — all three resume verdicts.
    #[test]
    fn scenario_10_resume_after_transient_write_reconciles_from_reads() {
        let journal_pending = || {
            let mut journal = Journal::new();
            journal.append(JournalEvent::Started {
                execution_target: ExecutionTarget::Mock,
            });
            journal.append(JournalEvent::IdentityRead);
            journal.append(JournalEvent::SnapshotRead);
            journal.append(JournalEvent::BackedUp);
            journal.append(JournalEvent::TransientWriteApplied {
                field: "beeper_off_flags",
                mask: SYSTEM_INIT_OFF_MASK,
            });
            journal
        };

        // (a) The transient value survived: continue to save and verify; the write is not
        // re-applied (exactly one TransientWriteApplied in the final checkpoints).
        let mut fc = MockFc::new(snapshot(PREV));
        fc.respond(&set_frame(DESIRED)).unwrap();
        let report = resume_beeper_lifecycle(
            ExecutionTarget::Mock,
            MockTransport::new(fc, InMemoryAudit::new()),
            meta(),
            approvals(),
            journal_pending(),
            backup_for_change(),
        );
        assert_eq!(report.terminal, TerminalClassification::CompletedVerified);
        assert_eq!(checkpoint_count(&report, "TransientWriteApplied"), 1);

        // (b) The transient value was lost: re-apply under fresh approval, then finish.
        let report = resume_beeper_lifecycle(
            ExecutionTarget::Mock,
            MockTransport::new(MockFc::new(snapshot(PREV)), InMemoryAudit::new()),
            meta(),
            approvals(),
            journal_pending(),
            backup_for_change(),
        );
        assert_eq!(report.terminal, TerminalClassification::CompletedVerified);
        assert_eq!(checkpoint_count(&report, "TransientWriteApplied"), 2);

        // (c) A third value: nothing is provable — state unknown, and no write is sent.
        let mut fc = MockFc::new(snapshot(PREV));
        fc.respond(&set_frame(0x0002_0000)).unwrap();
        let report = resume_beeper_lifecycle(
            ExecutionTarget::Mock,
            MockTransport::new(fc, InMemoryAudit::new()),
            meta(),
            approvals(),
            journal_pending(),
            backup_for_change(),
        );
        assert_eq!(
            report.terminal,
            TerminalClassification::StateUnknownRecoveryRequired
        );
        assert_eq!(
            report.failure_stage,
            Some(FailureStage::ResumeUnexpectedValue)
        );
        assert!(sent_write_commands(&report).is_empty());
    }

    /// 11. Restart between the save and the reboot: never assume the save's outcome.
    #[test]
    fn scenario_11_resume_after_save_verifies_through_a_reboot() {
        let journal_saved = || {
            let mut journal = Journal::new();
            journal.append(JournalEvent::Started {
                execution_target: ExecutionTarget::Mock,
            });
            journal.append(JournalEvent::IdentityRead);
            journal.append(JournalEvent::SnapshotRead);
            journal.append(JournalEvent::BackedUp);
            journal.append(JournalEvent::TransientWriteApplied {
                field: "beeper_off_flags",
                mask: SYSTEM_INIT_OFF_MASK,
            });
            journal.append(JournalEvent::ReReadBeforeSave);
            journal.append(JournalEvent::Saved);
            journal
        };

        // (a) The save really committed: the reboot-verify path completes verified.
        let report = resume_beeper_lifecycle(
            ExecutionTarget::Mock,
            MockTransport::new(MockFc::new(snapshot(DESIRED)), InMemoryAudit::new()),
            meta(),
            approvals(),
            journal_saved(),
            backup_for_change(),
        );
        assert_eq!(report.terminal, TerminalClassification::CompletedVerified);

        // (b) The save never committed: the post-reboot read disproves it and recovery
        // verifies the previous value in place.
        let mut fc = MockFc::new(snapshot(PREV));
        fc.respond(&set_frame(DESIRED)).unwrap(); // RAM=desired, EEPROM=previous
        let report = resume_beeper_lifecycle(
            ExecutionTarget::Mock,
            MockTransport::new(fc, InMemoryAudit::new()),
            meta(),
            approvals(),
            journal_saved(),
            backup_for_change(),
        );
        assert_eq!(report.terminal, TerminalClassification::CompletedRestored);
        assert!(report.recovery_evidence.unwrap().verified_in_place);
    }

    /// 12. Restart during case recording: the report rebuild is idempotent.
    #[test]
    fn scenario_12_resume_with_a_terminal_journal_is_idempotent() {
        let mut journal = Journal::new();
        for event in [
            JournalEvent::Started {
                execution_target: ExecutionTarget::Mock,
            },
            JournalEvent::IdentityRead,
            JournalEvent::SnapshotRead,
            JournalEvent::BackedUp,
            JournalEvent::TransientWriteApplied {
                field: "beeper_off_flags",
                mask: SYSTEM_INIT_OFF_MASK,
            },
            JournalEvent::ReReadBeforeSave,
            JournalEvent::Saved,
            JournalEvent::Rebooted,
            JournalEvent::Reconnected,
            JournalEvent::Verified,
        ] {
            journal.append(event);
        }
        let events_before = journal.events().len();
        let report = resume_beeper_lifecycle(
            ExecutionTarget::Mock,
            MockTransport::new(MockFc::new(snapshot(DESIRED)), InMemoryAudit::new()),
            meta(),
            approvals(),
            journal,
            backup_for_change(),
        );
        assert_eq!(report.terminal, TerminalClassification::CompletedVerified);
        // No duplicated events, no transport I/O at all.
        assert_eq!(report.checkpoints.len(), events_before);
        assert!(report.audit.is_empty());

        // An incomplete evidence chain can never fabricate a completed terminal.
        let mut truncated = Journal::new();
        truncated.append(JournalEvent::Verified);
        let report = resume_beeper_lifecycle(
            ExecutionTarget::Mock,
            MockTransport::new(MockFc::new(snapshot(DESIRED)), InMemoryAudit::new()),
            meta(),
            approvals(),
            truncated,
            backup_for_change(),
        );
        assert_eq!(
            report.terminal,
            TerminalClassification::StateUnknownRecoveryRequired
        );
        assert_eq!(
            report.failure_stage,
            Some(FailureStage::ResumeIncompleteEvidence)
        );
    }

    /// 13. Disconnect during identification: nothing was written, honest abort.
    #[test]
    fn scenario_13_disconnect_during_identification_aborts() {
        let report = run_with_faults(vec![(1, Fault::Disconnected)], PREV);
        assert_eq!(
            report.terminal,
            TerminalClassification::AbortedBeforeAnyWrite
        );
        assert_eq!(report.failure_stage, Some(FailureStage::Identify));
        assert!(sent_write_commands(&report).is_empty());
        assert_eq!(report.terminal_session_state, SessionState::Disconnected);
    }

    /// 14. Disconnect during the snapshot read.
    #[test]
    fn scenario_14_disconnect_during_snapshot_read_aborts() {
        let report = run_with_faults(vec![(ORD_SNAPSHOT, Fault::Disconnected)], PREV);
        assert_eq!(
            report.terminal,
            TerminalClassification::AbortedBeforeAnyWrite
        );
        assert_eq!(report.failure_stage, Some(FailureStage::SnapshotRead));
        assert!(sent_write_commands(&report).is_empty());
    }

    /// 15. Disconnect during the transient write: the write may not have arrived —
    ///     recovery reads first and proves it never did.
    #[test]
    fn scenario_15_disconnect_during_transient_write_reads_before_writing() {
        let report = run_with_faults(vec![(ORD_SET, Fault::Disconnected)], PREV);
        assert_eq!(report.terminal, TerminalClassification::CompletedRestored);
        assert!(report.recovery_evidence.unwrap().verified_in_place);
    }

    /// 16. Disconnect during the save.
    #[test]
    fn scenario_16_disconnect_during_save_recovers_to_restored() {
        let report = run_with_faults(vec![(ORD_SAVE, Fault::Disconnected)], PREV);
        assert_eq!(report.terminal, TerminalClassification::CompletedRestored);
        assert!(!report.recovery_evidence.unwrap().verified_in_place);
    }

    /// 17. Disconnect during the reboot request.
    #[test]
    fn scenario_17_disconnect_during_reboot_recovers_to_restored() {
        let report = run_with_faults(vec![(ORD_REBOOT, Fault::Disconnected)], PREV);
        assert_eq!(report.terminal, TerminalClassification::CompletedRestored);
        assert_eq!(report.failure_stage, Some(FailureStage::Reboot));
    }

    /// 18. Simulated power loss during the EEPROM write: the device reset and the save
    ///     never committed; recovery proves the previous value by reading.
    #[test]
    fn scenario_18_power_loss_during_eeprom_write_reads_before_writing() {
        let report = run_mock(PowerLossAtSave {
            fc: MockFc::new(snapshot(PREV)),
            fired: false,
        });
        assert_eq!(report.terminal, TerminalClassification::CompletedRestored);
        assert_eq!(report.failure_stage, Some(FailureStage::Save));
        assert!(report.recovery_evidence.unwrap().verified_in_place);
    }

    /// 19–21. Port busy / permission denied / missing driver at the first exchange.
    #[test]
    fn scenarios_19_to_21_port_and_driver_failures_abort_before_any_write() {
        for fault in [
            Fault::PortBusy,
            Fault::PermissionDenied,
            Fault::MissingDriver,
        ] {
            let report = run_with_faults(vec![(0, fault)], PREV);
            assert_eq!(
                report.terminal,
                TerminalClassification::AbortedBeforeAnyWrite,
                "{fault:?}",
            );
            assert!(sent_write_commands(&report).is_empty());
        }
    }

    /// 22. The recovery SET fails.
    #[test]
    fn scenario_22_recovery_set_failure_is_state_unknown() {
        let report = run_with_faults(
            vec![
                (ORD_SAVE, Fault::Timeout),
                (ORD_RECOVERY_SET, Fault::Timeout),
            ],
            PREV,
        );
        assert_eq!(
            report.terminal,
            TerminalClassification::StateUnknownRecoveryRequired
        );
        assert_eq!(
            report.recovery_evidence.unwrap().failed_stage,
            Some(ade_recovery::RecoveryStage::RestoreSet),
        );
    }

    /// 23. The recovery EEPROM save fails.
    #[test]
    fn scenario_23_recovery_save_failure_is_state_unknown() {
        let report = run_with_faults(
            vec![
                (ORD_SAVE, Fault::Timeout),
                (ORD_RECOVERY_SAVE, Fault::Timeout),
            ],
            PREV,
        );
        assert_eq!(
            report.terminal,
            TerminalClassification::StateUnknownRecoveryRequired
        );
        assert_eq!(
            report.recovery_evidence.unwrap().failed_stage,
            Some(ade_recovery::RecoveryStage::Save),
        );
    }

    /// 24. The recovery reboot fails.
    #[test]
    fn scenario_24_recovery_reboot_failure_is_state_unknown() {
        let report = run_with_faults(
            vec![
                (ORD_SAVE, Fault::Timeout),
                (ORD_RECOVERY_REBOOT, Fault::Disconnected),
            ],
            PREV,
        );
        assert_eq!(
            report.terminal,
            TerminalClassification::StateUnknownRecoveryRequired
        );
        assert_eq!(
            report.recovery_evidence.unwrap().failed_stage,
            Some(ade_recovery::RecoveryStage::Reboot),
        );
    }

    /// 25. The recovery re-identification meets a different device: recovery stops with
    ///     no recovery write.
    #[test]
    fn scenario_25_recovery_identity_mismatch_writes_nothing() {
        let responder = FaultInjector::new(
            SwapIdentityAfterCalls {
                fc: MockFc::new(snapshot(PREV)),
                n: ORD_SAVE,
                calls: 0,
            },
            ORD_SAVE,
            Fault::Timeout,
        );
        let report = run_mock(responder);
        assert_eq!(
            report.terminal,
            TerminalClassification::StateUnknownRecoveryRequired
        );
        let recovery = report.recovery_evidence.unwrap();
        assert_eq!(
            recovery.failed_stage,
            Some(ade_recovery::RecoveryStage::IdentityMismatch),
        );
        assert!(!recovery.identity_reproven);
        // After the failed save, only identification reads were sent — never a write.
        let post_failure_writes: Vec<u8> = report.audit[ORD_SAVE + 1..]
            .iter()
            .filter(|e| e.disposition == AuditDisposition::Sent)
            .map(|e| e.command)
            .filter(|c| CommandId::from_u8(*c).is_some_and(ade_transport::is_write_command))
            .collect();
        assert!(post_failure_writes.is_empty());
    }

    /// 26. The final state cannot be proven (the recovery verification read fails).
    #[test]
    fn scenario_26_unprovable_final_state_is_state_unknown() {
        let report = run_with_faults(
            vec![
                (ORD_SAVE, Fault::Timeout),
                (ORD_RECOVERY_FINAL_READ, Fault::Timeout),
            ],
            PREV,
        );
        assert_eq!(
            report.terminal,
            TerminalClassification::StateUnknownRecoveryRequired
        );
        assert_eq!(
            report.recovery_evidence.unwrap().failed_stage,
            Some(ade_recovery::RecoveryStage::FinalRead),
        );
        assert_eq!(report.readiness, "STATE UNKNOWN — RECOVERY REQUIRED");
    }

    // ---- additional invariants ----

    /// An MSP error reply is a command failure both on the happy path and inside recovery.
    #[test]
    fn an_error_reply_is_never_success_anywhere() {
        let mut fc = MockFc::new(snapshot(PREV));
        fc.set_armed(true); // every EEPROM write is refused with an error frame
        let report = run_mock(fc);
        assert_eq!(report.failure_stage, Some(FailureStage::Save));
        // Recovery's own save also fails against the armed mock: state unknown, honestly.
        assert_eq!(
            report.terminal,
            TerminalClassification::StateUnknownRecoveryRequired
        );
        assert_eq!(
            report.recovery_evidence.unwrap().failed_stage,
            Some(ade_recovery::RecoveryStage::Save),
        );
    }

    /// An unsolicited reply never advances the lifecycle as success.
    #[test]
    fn an_unsolicited_reply_never_advances_the_lifecycle() {
        let report = run_mock(UnsolicitedFirstReply {
            fc: MockFc::new(snapshot(PREV)),
            calls: 0,
        });
        assert_eq!(
            report.terminal,
            TerminalClassification::AbortedBeforeAnyWrite
        );
        assert!(sent_write_commands(&report).is_empty());
    }

    /// Every terminal path produces a report with the markers, and MOCK/REPLAY exercise
    /// labels never claim hardware observation.
    #[test]
    fn every_terminal_path_reports_with_markers_and_without_hardware_claims() {
        let reports = [
            run_mock(MockFc::new(snapshot(PREV))),
            run_mock(MockFc::new(snapshot(DESIRED))),
            run_with_faults(vec![(0, Fault::PortBusy)], PREV),
            run_with_faults(vec![(ORD_SAVE, Fault::Timeout)], PREV),
            run_mock(DeadAfterReboot {
                fc: MockFc::new(snapshot(PREV)),
                dead: false,
            }),
        ];
        for report in reports {
            assert_eq!(report.markers, REPORT_MARKERS);
            assert_ne!(report.verification_state, "HARDWARE_OBSERVED");
            assert!(!format!("{report:?}").contains("HARDWARE_OBSERVED"));
            assert!(report.case.terminal_state.is_some());
        }
    }

    /// DShot fields stay untouched before the save, after the reboot and after recovery.
    #[test]
    fn dshot_fields_are_untouched_on_every_path() {
        let verified = run_mock(MockFc::new(snapshot(PREV)));
        let evidence = verified.verification_evidence.unwrap();
        assert_eq!(evidence.observed_dshot_tone, TONE);
        assert_eq!(evidence.observed_dshot_off_flags, DSHOT_FLAGS);

        let restored = run_with_faults(vec![(ORD_SAVE, Fault::Timeout)], PREV);
        let final_snapshot = restored.recovery_evidence.unwrap().final_snapshot.unwrap();
        assert_eq!(final_snapshot.dshot_beacon_tone, TONE);
        assert_eq!(final_snapshot.dshot_beacon_off_flags, DSHOT_FLAGS);
    }
}
