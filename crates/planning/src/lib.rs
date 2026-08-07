#![forbid(unsafe_code)]

//! # `ade-planning` — product goals and the M1 beeper change plan
//!
//! [`product`] fixes the user/program responsibility boundary and provides protocol-neutral,
//! provenance- and recovery-bearing setting deltas for every future plan. It cannot emit
//! protocol frames or authorise writes. The existing M1 implementation builds the single
//! typed beeper plan: flip exactly the
//! `BEEPER_SYSTEM_INIT` bit of `beeper_off_flags`, or prove that nothing needs to change
//! (an explicit no-op plan). The plan carries the full initial snapshot, the exact intended
//! delta, the ordered steps with their [`WriteCommandClass`] and [`RecoveryClass`], the
//! verification requirements, the recovery path and the provenance records it relies on.
//!
//! A plan whose delta touches any bit other than the intended mask is rejected — there is no
//! way to smuggle a wider change through this planner.

pub mod product;

use ade_protocol_msp::{BeeperConfigSnapshot, SYSTEM_INIT_OFF_MASK};
use ade_safety::{RecoveryClass, WriteCommandClass};

/// The desired state of the power-on initialisation beep.
///
/// The recorded fact (`bf-4.5.5-beeper-system-init-bit`): in `beeper_off_flags` a **set** bit
/// disables its condition and a **clear** bit allows it; `BEEPER_SYSTEM_INIT` is
/// `1 << 16` = `0x0001_0000`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemInitBeeperGoal {
    /// The power-on beep must be allowed (mask bit clear).
    Enable,
    /// The power-on beep must be disabled (mask bit set).
    Disable,
}

impl SystemInitBeeperGoal {
    /// The `beeper_off_flags` value that realises this goal from `previous`, touching only
    /// the SYSTEM_INIT bit.
    #[must_use]
    pub const fn desired_off_flags(self, previous: u32) -> u32 {
        match self {
            SystemInitBeeperGoal::Enable => previous & !SYSTEM_INIT_OFF_MASK,
            SystemInitBeeperGoal::Disable => previous | SYSTEM_INIT_OFF_MASK,
        }
    }
}

/// One ordered step of the plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanStepKind {
    /// Send the exact four-byte `MSP_SET_BEEPER_CONFIG` (transient, RAM only).
    ApplyTransientSet,
    /// Re-read the full snapshot before saving.
    ReReadBeforeSave,
    /// Commit to persistent storage (`MSP_EEPROM_WRITE`).
    SaveEeprom,
    /// Reboot the device (`MSP_REBOOT`, empty payload).
    Reboot,
    /// Re-identify after the reboot and require the same composite identity.
    ReIdentify,
    /// Final full re-read and verification of the intended bit only.
    VerifyRead,
}

/// A planned step with its declared command class and recovery class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedStep {
    /// What the step does.
    pub kind: PlanStepKind,
    /// The write-command class the executor must enforce for it.
    pub class: WriteCommandClass,
    /// The recovery class declared for it (reads carry `NotApplicableNoWrite`).
    pub recovery: RecoveryClass,
}

/// An alternative that was considered and rejected, with the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RejectedAlternative {
    /// Short name of the alternative.
    pub name: &'static str,
    /// Why it is rejected in M1.
    pub reason: &'static str,
}

/// What the final verification must prove.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerificationRequirements {
    /// The single bit that is allowed to change.
    pub intended_mask: u32,
    /// The exact `beeper_off_flags` value that must be observed.
    pub expected_off_flags: u32,
    /// The DShot beacon tone must still equal this value.
    pub required_dshot_tone: u8,
    /// The DShot beacon off-flags must still equal this value.
    pub required_dshot_off_flags: u32,
    /// All bits outside `intended_mask` must equal `expected_off_flags` on these bits too
    /// (spelled out so the check cannot silently narrow).
    pub unchanged_bits_mask: u32,
}

/// The recovery path declared by the plan, executed only from the pre-change backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryPathSpec {
    /// The `beeper_off_flags` value recovery restores (the pre-change value).
    pub restore_off_flags: u32,
    /// Recovery class of the restore write.
    pub set_recovery: RecoveryClass,
    /// Recovery class of the restore save.
    pub save_recovery: RecoveryClass,
    /// Recovery class of the recovery reboot.
    pub reboot_recovery: RecoveryClass,
}

/// Why a plan could not be built or fails validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanError {
    /// The delta would change bits outside the intended mask.
    UnintendedBitsWouldChange {
        /// The full set of changed bits.
        changed_bits: u32,
        /// The only permitted mask.
        intended_mask: u32,
    },
    /// The plan claims to be a no-op but its delta is not empty (or vice versa).
    NoOpInconsistent,
}

/// The provenance records this plan relies on, by record id.
pub const PLAN_PROVENANCE_RECORDS: [&str; 8] = [
    "bf-4.5.5-mspv1-frame",
    "bf-4.5.5-msp-api-version",
    "bf-4.5.5-msp-fc-variant",
    "bf-4.5.5-msp-fc-version",
    "bf-4.5.5-msp-board-info",
    "bf-4.5.5-msp-beeper-config",
    "bf-4.5.5-msp-set-beeper-config",
    "bf-4.5.5-beeper-system-init-bit",
];

/// The alternatives considered and rejected for the M1 slice.
pub const REJECTED_ALTERNATIVES: [RejectedAlternative; 3] = [
    RejectedAlternative {
        name: "cli-beeper-command",
        reason: "the CLI surface is out of M1 scope and not covered by a pinned record",
    },
    RejectedAlternative {
        name: "full-nine-byte-set-write",
        reason: "would rewrite the DShot beacon fields; M1 must never touch them",
    },
    RejectedAlternative {
        name: "multi-bit-flag-rewrite",
        reason: "any write whose delta exceeds the single SYSTEM_INIT bit is forbidden",
    },
];

/// The complete typed plan for one beeper change (or an explicit no-op).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeeperPlan {
    /// The full snapshot the plan was built from.
    pub initial_snapshot: BeeperConfigSnapshot,
    /// The goal.
    pub goal: SystemInitBeeperGoal,
    /// `beeper_off_flags` before the change.
    pub previous_off_flags: u32,
    /// `beeper_off_flags` the plan intends to write.
    pub desired_off_flags: u32,
    /// The exact changed bits (`previous ^ desired`).
    pub changed_bits: u32,
    /// The SYSTEM_INIT mask (`0x0001_0000`).
    pub system_init_mask: u32,
    /// The DShot beacon tone that must remain untouched.
    pub untouched_dshot_tone: u8,
    /// The DShot beacon off-flags that must remain untouched.
    pub untouched_dshot_off_flags: u32,
    /// Whether the desired state is already present (explicit no-op).
    pub is_noop: bool,
    /// The alternatives that were rejected.
    pub rejected_alternatives: Vec<RejectedAlternative>,
    /// The ordered steps (empty write set for a no-op: verification read only).
    pub steps: Vec<PlannedStep>,
    /// What verification must prove.
    pub verification: VerificationRequirements,
    /// The declared recovery path.
    pub recovery_path: RecoveryPathSpec,
    /// Provenance record ids the plan relies on.
    pub provenance_records: Vec<&'static str>,
}

impl BeeperPlan {
    /// Re-validate the plan's invariants. Called again immediately before execution so a
    /// tampered plan can never reach the executor.
    ///
    /// # Errors
    /// [`PlanError`] if the delta touches any bit outside the mask, or the no-op flag is
    /// inconsistent with the delta.
    pub fn validate(&self) -> Result<(), PlanError> {
        let changed = self.previous_off_flags ^ self.desired_off_flags;
        if changed != self.changed_bits {
            return Err(PlanError::UnintendedBitsWouldChange {
                changed_bits: changed,
                intended_mask: self.system_init_mask,
            });
        }
        match (self.is_noop, changed) {
            (true, 0) => Ok(()),
            (false, c) if c == self.system_init_mask => Ok(()),
            (true, _) | (false, 0) => Err(PlanError::NoOpInconsistent),
            (false, c) => Err(PlanError::UnintendedBitsWouldChange {
                changed_bits: c,
                intended_mask: self.system_init_mask,
            }),
        }
    }

    /// The write/reboot steps of the plan (empty for a no-op).
    #[must_use]
    pub fn write_steps(&self) -> Vec<PlannedStep> {
        self.steps
            .iter()
            .copied()
            .filter(|s| !matches!(s.class, WriteCommandClass::NoWrite))
            .collect()
    }
}

/// Build the plan for `goal` from the observed `initial` snapshot.
///
/// The desired value is derived from the goal by touching only the SYSTEM_INIT bit, so
/// `previous ^ desired` is always `0` (no-op) or exactly [`SYSTEM_INIT_OFF_MASK`]; the
/// result is still validated before being returned.
///
/// # Errors
/// [`PlanError`] if validation fails (structurally unreachable from this constructor, but
/// the check is real and also guards deserialised or modified plans).
pub fn build_plan(
    initial: &BeeperConfigSnapshot,
    goal: SystemInitBeeperGoal,
) -> Result<BeeperPlan, PlanError> {
    let previous = initial.beeper_off_flags;
    let desired = goal.desired_off_flags(previous);
    let changed = previous ^ desired;
    let is_noop = changed == 0;

    let read = |kind| PlannedStep {
        kind,
        class: WriteCommandClass::NoWrite,
        recovery: RecoveryClass::NotApplicableNoWrite,
    };
    let steps = if is_noop {
        vec![read(PlanStepKind::VerifyRead)]
    } else {
        vec![
            PlannedStep {
                kind: PlanStepKind::ApplyTransientSet,
                class: WriteCommandClass::TransientConfig,
                recovery: RecoveryClass::TransientWritePendingReconcileOnResume,
            },
            read(PlanStepKind::ReReadBeforeSave),
            PlannedStep {
                kind: PlanStepKind::SaveEeprom,
                class: WriteCommandClass::PersistentConfig,
                recovery: RecoveryClass::AutomaticRollbackSupported,
            },
            PlannedStep {
                kind: PlanStepKind::Reboot,
                class: WriteCommandClass::Reboot,
                recovery: RecoveryClass::ManualRecoveryRequired,
            },
            read(PlanStepKind::ReIdentify),
            read(PlanStepKind::VerifyRead),
        ]
    };

    let plan = BeeperPlan {
        initial_snapshot: initial.clone(),
        goal,
        previous_off_flags: previous,
        desired_off_flags: desired,
        changed_bits: changed,
        system_init_mask: SYSTEM_INIT_OFF_MASK,
        untouched_dshot_tone: initial.dshot_beacon_tone,
        untouched_dshot_off_flags: initial.dshot_beacon_off_flags,
        is_noop,
        rejected_alternatives: REJECTED_ALTERNATIVES.to_vec(),
        steps,
        verification: VerificationRequirements {
            intended_mask: SYSTEM_INIT_OFF_MASK,
            expected_off_flags: desired,
            required_dshot_tone: initial.dshot_beacon_tone,
            required_dshot_off_flags: initial.dshot_beacon_off_flags,
            unchanged_bits_mask: !SYSTEM_INIT_OFF_MASK,
        },
        recovery_path: RecoveryPathSpec {
            restore_off_flags: previous,
            set_recovery: RecoveryClass::RestoreFromBackupSupported,
            save_recovery: RecoveryClass::RestoreFromBackupSupported,
            reboot_recovery: RecoveryClass::ManualRecoveryRequired,
        },
        provenance_records: PLAN_PROVENANCE_RECORDS.to_vec(),
    };
    plan.validate()?;
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(off_flags: u32) -> BeeperConfigSnapshot {
        BeeperConfigSnapshot {
            beeper_off_flags: off_flags,
            dshot_beacon_tone: 2,
            dshot_beacon_off_flags: 0x0000_0004,
        }
    }

    #[test]
    fn a_disable_plan_flips_exactly_the_system_init_bit() {
        let plan = build_plan(&snapshot(0x0000_0005), SystemInitBeeperGoal::Disable).unwrap();
        assert_eq!(plan.previous_off_flags, 0x0000_0005);
        assert_eq!(plan.desired_off_flags, 0x0001_0005);
        assert_eq!(
            plan.previous_off_flags ^ plan.desired_off_flags,
            SYSTEM_INIT_OFF_MASK,
            "the delta is exactly the SYSTEM_INIT mask",
        );
        assert_eq!(plan.changed_bits, SYSTEM_INIT_OFF_MASK);
        assert!(!plan.is_noop);
        assert_eq!(plan.untouched_dshot_tone, 2);
        assert_eq!(plan.untouched_dshot_off_flags, 0x0000_0004);
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn an_enable_plan_clears_the_bit_and_touches_nothing_else() {
        let plan = build_plan(&snapshot(0x0001_00A0), SystemInitBeeperGoal::Enable).unwrap();
        assert_eq!(plan.desired_off_flags, 0x0000_00A0);
        assert_eq!(plan.changed_bits, SYSTEM_INIT_OFF_MASK);
        assert!(!plan.is_noop);
    }

    #[test]
    fn a_noop_plan_is_explicit_and_carries_no_write_steps() {
        // Disable requested but the bit is already set: explicit no-op.
        let plan = build_plan(&snapshot(0x0001_0000), SystemInitBeeperGoal::Disable).unwrap();
        assert!(plan.is_noop);
        assert_eq!(plan.changed_bits, 0);
        assert!(
            plan.write_steps().is_empty(),
            "no SET, no EEPROM, no reboot"
        );
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].kind, PlanStepKind::VerifyRead);
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn the_ordered_steps_declare_the_correct_classes_and_recoveries() {
        let plan = build_plan(&snapshot(0), SystemInitBeeperGoal::Disable).unwrap();
        let kinds: Vec<_> = plan.steps.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            vec![
                PlanStepKind::ApplyTransientSet,
                PlanStepKind::ReReadBeforeSave,
                PlanStepKind::SaveEeprom,
                PlanStepKind::Reboot,
                PlanStepKind::ReIdentify,
                PlanStepKind::VerifyRead,
            ],
        );
        assert_eq!(plan.steps[0].class, WriteCommandClass::TransientConfig);
        assert_eq!(
            plan.steps[0].recovery,
            RecoveryClass::TransientWritePendingReconcileOnResume
        );
        assert_eq!(plan.steps[2].class, WriteCommandClass::PersistentConfig);
        assert_eq!(
            plan.steps[2].recovery,
            RecoveryClass::AutomaticRollbackSupported
        );
        assert_eq!(plan.steps[3].class, WriteCommandClass::Reboot);
        assert_eq!(
            plan.steps[3].recovery,
            RecoveryClass::ManualRecoveryRequired
        );
        // Reads carry no recovery class.
        for step in [plan.steps[1], plan.steps[4], plan.steps[5]] {
            assert_eq!(step.class, WriteCommandClass::NoWrite);
            assert_eq!(step.recovery, RecoveryClass::NotApplicableNoWrite);
        }
    }

    #[test]
    fn a_tampered_plan_with_unintended_bits_is_rejected() {
        let mut plan = build_plan(&snapshot(0), SystemInitBeeperGoal::Disable).unwrap();
        // Tamper: widen the delta beyond the mask.
        plan.desired_off_flags |= 0x0000_0002;
        plan.changed_bits = plan.previous_off_flags ^ plan.desired_off_flags;
        assert_eq!(
            plan.validate(),
            Err(PlanError::UnintendedBitsWouldChange {
                changed_bits: SYSTEM_INIT_OFF_MASK | 0x0000_0002,
                intended_mask: SYSTEM_INIT_OFF_MASK,
            }),
        );
        // Tamper: stale changed_bits that hides the widened delta.
        let mut plan2 = build_plan(&snapshot(0), SystemInitBeeperGoal::Disable).unwrap();
        plan2.desired_off_flags |= 0x0000_0002;
        assert!(plan2.validate().is_err());
    }

    #[test]
    fn verification_and_recovery_specs_are_complete() {
        let plan = build_plan(&snapshot(0x40), SystemInitBeeperGoal::Disable).unwrap();
        assert_eq!(plan.verification.intended_mask, SYSTEM_INIT_OFF_MASK);
        assert_eq!(plan.verification.expected_off_flags, 0x0001_0040);
        assert_eq!(plan.verification.required_dshot_tone, 2);
        assert_eq!(plan.verification.required_dshot_off_flags, 0x0000_0004);
        assert_eq!(plan.verification.unchanged_bits_mask, !SYSTEM_INIT_OFF_MASK);
        assert_eq!(plan.recovery_path.restore_off_flags, 0x40);
        assert_eq!(
            plan.recovery_path.set_recovery,
            RecoveryClass::RestoreFromBackupSupported
        );
        assert_eq!(
            plan.recovery_path.save_recovery,
            RecoveryClass::RestoreFromBackupSupported
        );
        assert_eq!(
            plan.recovery_path.reboot_recovery,
            RecoveryClass::ManualRecoveryRequired
        );
        assert!(!plan.rejected_alternatives.is_empty());
        assert!(
            plan.provenance_records
                .contains(&"bf-4.5.5-beeper-system-init-bit")
        );
    }
}
