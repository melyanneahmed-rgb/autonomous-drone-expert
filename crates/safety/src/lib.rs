#![forbid(unsafe_code)]

//! # `ade-safety` — write authority, recovery classes and the hardware gate (M1)
//!
//! This crate owns the **write-authority** contract. Its central guarantee is structural,
//! not procedural: a write against the [`ExecutionTarget::Hardware`] target can **never** be
//! authorised in M1, because the only value that grants a write, [`WriteApproval`], is
//! impossible to construct for `Hardware` and has no public constructor of its own.

/// Where a plan executes. Mock and Replay are simulations; Hardware is a real board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionTarget {
    /// Deterministic in-memory model of a flight controller.
    Mock,
    /// Replay against a project-owned transcript.
    Replay,
    /// A real flight controller. Structurally blocked from writes in M1.
    Hardware,
}

/// The class of an outbound command with respect to persistence and effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteCommandClass {
    /// A read/identify command — not a write at all.
    NoWrite,
    /// A RAM-only change that is lost on reboot unless committed (e.g. set beeper config).
    TransientConfig,
    /// A commit of configuration to persistent storage (EEPROM).
    PersistentConfig,
    /// A reboot request — changes no configuration itself.
    Reboot,
}

impl WriteCommandClass {
    /// Whether this class actually mutates the device (transient or persistent).
    #[must_use]
    pub const fn is_write(self) -> bool {
        matches!(
            self,
            WriteCommandClass::TransientConfig | WriteCommandClass::PersistentConfig
        )
    }
}

/// The recovery posture that applies to a write (ADR-0005/0006). Every write carries one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryClass {
    /// The step performs no write; recovery is not applicable.
    NotApplicableNoWrite,
    /// A transient write not yet committed; on resume it is reconciled, never "rolled back".
    TransientWritePendingReconcileOnResume,
    /// The previous value can be restored automatically while the session stays provable.
    AutomaticRollbackSupported,
    /// Local state is lost but a backup can restore the previous value.
    RestoreFromBackupSupported,
    /// Reconnection or manual intervention is required; no automatic recovery.
    ManualRecoveryRequired,
    /// The terminal, honest failure: the state cannot be proven.
    StateUnknownRecoveryRequired,
}

/// The stable marker returned whenever a hardware write is requested in M1.
pub const HARDWARE_WRITE_GATE_NOT_APPROVED: &str = "HARDWARE_WRITE_GATE_NOT_APPROVED";

/// Why a write could not be authorised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteGateError {
    /// The hardware-write gate is not approved in M1. Carries the stable marker.
    HardwareGateNotApproved(&'static str),
    /// The declared recovery class is not compatible with the write class (see
    /// [`authorize_write`] for the full compatibility matrix). This also covers a write
    /// declared with [`RecoveryClass::NotApplicableNoWrite`] and any attempt to authorise a
    /// command under [`RecoveryClass::StateUnknownRecoveryRequired`].
    IncompatibleRecoveryClass,
    /// A non-write class was submitted through the write-authorisation path.
    NotAWrite,
}

/// Proof that a specific write is authorised against a **simulation** target.
///
/// There is no public constructor. The only way to obtain one is [`authorize_write`], which
/// never returns it for [`ExecutionTarget::Hardware`]. Downstream execution requires a value
/// of this type to emit a write, so a hardware write is unrepresentable, not merely refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteApproval {
    target: ExecutionTarget,
    class: WriteCommandClass,
    recovery: RecoveryClass,
}

impl WriteApproval {
    /// The simulation target this approval is bound to (never `Hardware`).
    #[must_use]
    pub const fn target(&self) -> ExecutionTarget {
        self.target
    }

    /// The write class this approval covers.
    #[must_use]
    pub const fn class(&self) -> WriteCommandClass {
        self.class
    }

    /// The recovery class declared for this write.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryClass {
        self.recovery
    }
}

/// Whether `recovery` is a legitimate recovery posture for a command of `class`.
///
/// This is the compatibility matrix every write is checked against. Each write/reboot class
/// admits only the recovery classes that can actually restore the device from that kind of
/// change:
///
/// | class              | permitted recovery classes                                         |
/// |--------------------|--------------------------------------------------------------------|
/// | `TransientConfig`  | `TransientWritePendingReconcileOnResume`, `RestoreFromBackupSupported` |
/// | `PersistentConfig` | `AutomaticRollbackSupported`, `RestoreFromBackupSupported`          |
/// | `Reboot`           | `ManualRecoveryRequired`                                           |
///
/// Consequences of this table (all rejected):
/// * `NotApplicableNoWrite` is never valid for a real write or a reboot;
/// * `StateUnknownRecoveryRequired` can never authorise any command;
/// * `ManualRecoveryRequired` is rejected for a config write (it is only for a reboot);
/// * `AutomaticRollbackSupported` is rejected for a reboot.
const fn recovery_is_compatible(class: WriteCommandClass, recovery: RecoveryClass) -> bool {
    match class {
        // A read is not a write and never reaches this check.
        WriteCommandClass::NoWrite => false,
        WriteCommandClass::TransientConfig => matches!(
            recovery,
            RecoveryClass::TransientWritePendingReconcileOnResume
                | RecoveryClass::RestoreFromBackupSupported
        ),
        WriteCommandClass::PersistentConfig => matches!(
            recovery,
            RecoveryClass::AutomaticRollbackSupported | RecoveryClass::RestoreFromBackupSupported
        ),
        WriteCommandClass::Reboot => matches!(recovery, RecoveryClass::ManualRecoveryRequired),
    }
}

/// Authorise a write. Returns [`WriteApproval`] only for a simulation target with a real
/// write class and a recovery class that is compatible with it (see [`recovery_is_compatible`]
/// for the matrix). A hardware target is always refused with
/// [`HARDWARE_WRITE_GATE_NOT_APPROVED`], and that refusal is checked **first**.
///
/// # Errors
/// - [`WriteGateError::HardwareGateNotApproved`] for [`ExecutionTarget::Hardware`] (checked
///   before anything else);
/// - [`WriteGateError::NotAWrite`] for [`WriteCommandClass::NoWrite`];
/// - [`WriteGateError::IncompatibleRecoveryClass`] if `recovery` is not permitted for `class`.
pub fn authorize_write(
    target: ExecutionTarget,
    class: WriteCommandClass,
    recovery: RecoveryClass,
) -> Result<WriteApproval, WriteGateError> {
    if matches!(target, ExecutionTarget::Hardware) {
        return Err(WriteGateError::HardwareGateNotApproved(
            HARDWARE_WRITE_GATE_NOT_APPROVED,
        ));
    }
    if matches!(class, WriteCommandClass::NoWrite) {
        return Err(WriteGateError::NotAWrite);
    }
    if !recovery_is_compatible(class, recovery) {
        return Err(WriteGateError::IncompatibleRecoveryClass);
    }
    Ok(WriteApproval {
        target,
        class,
        recovery,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_write_is_always_refused_with_the_stable_marker() {
        for class in [
            WriteCommandClass::TransientConfig,
            WriteCommandClass::PersistentConfig,
            WriteCommandClass::Reboot,
        ] {
            let result = authorize_write(
                ExecutionTarget::Hardware,
                class,
                RecoveryClass::AutomaticRollbackSupported,
            );
            assert_eq!(
                result,
                Err(WriteGateError::HardwareGateNotApproved(
                    HARDWARE_WRITE_GATE_NOT_APPROVED
                )),
            );
        }
    }

    #[test]
    fn mock_transient_write_with_recovery_is_authorised() {
        let approval = authorize_write(
            ExecutionTarget::Mock,
            WriteCommandClass::TransientConfig,
            RecoveryClass::TransientWritePendingReconcileOnResume,
        )
        .expect("simulation write is authorised");
        assert_eq!(approval.target(), ExecutionTarget::Mock);
        assert_eq!(approval.class(), WriteCommandClass::TransientConfig);
    }

    /// Every (class, recovery) pair the matrix accepts, and it authorises on a simulation
    /// target.
    #[test]
    fn every_compatible_pair_is_authorised() {
        let accepted = [
            (
                WriteCommandClass::TransientConfig,
                RecoveryClass::TransientWritePendingReconcileOnResume,
            ),
            (
                WriteCommandClass::TransientConfig,
                RecoveryClass::RestoreFromBackupSupported,
            ),
            (
                WriteCommandClass::PersistentConfig,
                RecoveryClass::AutomaticRollbackSupported,
            ),
            (
                WriteCommandClass::PersistentConfig,
                RecoveryClass::RestoreFromBackupSupported,
            ),
            (
                WriteCommandClass::Reboot,
                RecoveryClass::ManualRecoveryRequired,
            ),
        ];
        for (class, recovery) in accepted {
            let approval = authorize_write(ExecutionTarget::Mock, class, recovery)
                .unwrap_or_else(|e| panic!("expected {class:?}/{recovery:?} to authorise: {e:?}"));
            assert_eq!(approval.class(), class);
            assert_eq!(approval.recovery(), recovery);
        }
    }

    /// Representative incompatible pairs, one for each rejection the matrix must enforce.
    #[test]
    fn every_incompatible_pair_is_refused() {
        let rejected = [
            // NotApplicableNoWrite is never valid for a real write or a reboot.
            (
                WriteCommandClass::TransientConfig,
                RecoveryClass::NotApplicableNoWrite,
            ),
            (
                WriteCommandClass::PersistentConfig,
                RecoveryClass::NotApplicableNoWrite,
            ),
            (
                WriteCommandClass::Reboot,
                RecoveryClass::NotApplicableNoWrite,
            ),
            // StateUnknownRecoveryRequired can never authorise any command.
            (
                WriteCommandClass::TransientConfig,
                RecoveryClass::StateUnknownRecoveryRequired,
            ),
            (
                WriteCommandClass::PersistentConfig,
                RecoveryClass::StateUnknownRecoveryRequired,
            ),
            (
                WriteCommandClass::Reboot,
                RecoveryClass::StateUnknownRecoveryRequired,
            ),
            // ManualRecoveryRequired is only for a reboot, never a config write.
            (
                WriteCommandClass::TransientConfig,
                RecoveryClass::ManualRecoveryRequired,
            ),
            (
                WriteCommandClass::PersistentConfig,
                RecoveryClass::ManualRecoveryRequired,
            ),
            // AutomaticRollbackSupported is not a recovery posture for a reboot.
            (
                WriteCommandClass::Reboot,
                RecoveryClass::AutomaticRollbackSupported,
            ),
            // A transient write cannot claim automatic rollback (that is the persistent path).
            (
                WriteCommandClass::TransientConfig,
                RecoveryClass::AutomaticRollbackSupported,
            ),
            // A persistent write cannot claim the transient-reconcile posture.
            (
                WriteCommandClass::PersistentConfig,
                RecoveryClass::TransientWritePendingReconcileOnResume,
            ),
        ];
        for (class, recovery) in rejected {
            assert_eq!(
                authorize_write(ExecutionTarget::Replay, class, recovery),
                Err(WriteGateError::IncompatibleRecoveryClass),
                "expected {class:?}/{recovery:?} to be refused",
            );
        }
    }

    #[test]
    fn hardware_refusal_precedes_recovery_class_checks() {
        // Even an otherwise-incompatible pair on hardware reports the hardware gate first.
        assert_eq!(
            authorize_write(
                ExecutionTarget::Hardware,
                WriteCommandClass::Reboot,
                RecoveryClass::AutomaticRollbackSupported,
            ),
            Err(WriteGateError::HardwareGateNotApproved(
                HARDWARE_WRITE_GATE_NOT_APPROVED
            )),
        );
    }

    #[test]
    fn a_non_write_cannot_be_authorised_as_a_write() {
        assert_eq!(
            authorize_write(
                ExecutionTarget::Mock,
                WriteCommandClass::NoWrite,
                RecoveryClass::NotApplicableNoWrite,
            ),
            Err(WriteGateError::NotAWrite),
        );
    }
}
