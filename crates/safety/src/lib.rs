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
    /// A write class was declared with no recovery class (or vice versa).
    MissingRecoveryClass,
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

/// Authorise a write. Returns [`WriteApproval`] only for a simulation target with a real
/// write class and a matching recovery class. A hardware target is always refused with
/// [`HARDWARE_WRITE_GATE_NOT_APPROVED`].
///
/// # Errors
/// - [`WriteGateError::HardwareGateNotApproved`] for [`ExecutionTarget::Hardware`];
/// - [`WriteGateError::NotAWrite`] for [`WriteCommandClass::NoWrite`];
/// - [`WriteGateError::MissingRecoveryClass`] if the recovery class is
///   [`RecoveryClass::NotApplicableNoWrite`] for an actual write.
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
    // A reboot is not a config write but still flows through the gate; only genuine config
    // writes must carry a non-trivial recovery class.
    if class.is_write() && matches!(recovery, RecoveryClass::NotApplicableNoWrite) {
        return Err(WriteGateError::MissingRecoveryClass);
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

    #[test]
    fn a_config_write_without_a_recovery_class_is_refused() {
        assert_eq!(
            authorize_write(
                ExecutionTarget::Replay,
                WriteCommandClass::PersistentConfig,
                RecoveryClass::NotApplicableNoWrite,
            ),
            Err(WriteGateError::MissingRecoveryClass),
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
