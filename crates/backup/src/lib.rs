#![forbid(unsafe_code)]

//! # `ade-backup` — the first (deliberately limited) M1 backup
//!
//! The first backup (ADR-0006) captures exactly what recovery needs and nothing that could
//! track a person or a unit. By construction this schema has **no** field for a serial
//! number, USB UID, GPS coordinate, home position, network name or any other stable tracking
//! identifier — those are simply not representable here.

use ade_facts::DeviceIdentity;
use ade_protocol_msp::BeeperConfigSnapshot;
use ade_safety::RecoveryClass;
use ade_transport::AuditEntry;

/// The current schema version of a [`Backup`].
pub const BACKUP_SCHEMA_VERSION: u32 = 1;

/// The single bit-level change the slice intends to make.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesiredDelta {
    /// The configuration field being changed (always `beeper_off_flags` in M1).
    pub field: &'static str,
    /// The mask of the single bit being changed.
    pub mask: u32,
    /// The previous value of the whole field.
    pub previous: u32,
    /// The intended new value of the whole field.
    pub next: u32,
}

impl DesiredDelta {
    /// The bits that differ between `previous` and `next` (should equal `mask`).
    #[must_use]
    pub const fn changed_bits(&self) -> u32 {
        self.previous ^ self.next
    }

    /// Whether exactly the intended bit — and no other — changes.
    #[must_use]
    pub const fn touches_only_intended_bit(&self) -> bool {
        self.changed_bits() == self.mask
    }
}

/// The first M1 backup. Only recovery-relevant, non-identifying fields are present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    /// Schema version.
    pub schema_version: u32,
    /// Board and firmware identity (model-level, never a unit serial).
    pub identity: DeviceIdentity,
    /// The full beeper snapshot read before the change.
    pub beeper_snapshot: BeeperConfigSnapshot,
    /// The intended delta.
    pub desired_delta: DesiredDelta,
    /// The outbound frame audit log (metadata only).
    pub audit: Vec<AuditEntry>,
    /// Lifecycle checkpoints reached (named).
    pub checkpoints: Vec<String>,
    /// The recovery classification for the write.
    pub recovery_class: RecoveryClass,
    /// Provenance record identifiers the plan relied on.
    pub provenance_refs: Vec<String>,
}

impl Backup {
    /// Build the first backup for a beeper change.
    #[must_use]
    pub fn new(
        identity: DeviceIdentity,
        beeper_snapshot: BeeperConfigSnapshot,
        desired_delta: DesiredDelta,
        recovery_class: RecoveryClass,
    ) -> Self {
        Self {
            schema_version: BACKUP_SCHEMA_VERSION,
            identity,
            beeper_snapshot,
            desired_delta,
            audit: Vec::new(),
            checkpoints: Vec::new(),
            recovery_class,
            provenance_refs: Vec::new(),
        }
    }

    /// The value to restore the field to on rollback (the pre-change value).
    #[must_use]
    pub const fn restore_value(&self) -> u32 {
        self.desired_delta.previous
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ade_protocol_msp::{
        ApiVersion, BeeperConfigSnapshot, FcVariant, FcVersion, SYSTEM_INIT_OFF_MASK,
    };

    fn identity() -> DeviceIdentity {
        DeviceIdentity {
            api: ApiVersion {
                protocol_version: 0,
                api_major: 1,
                api_minor: 46,
            },
            variant: FcVariant {
                identifier: *b"BTFL",
            },
            version: FcVersion {
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
            mcu_type_id: 0x1B,
        }
    }

    #[test]
    fn the_delta_touches_only_the_intended_bit() {
        let delta = DesiredDelta {
            field: "beeper_off_flags",
            mask: SYSTEM_INIT_OFF_MASK,
            previous: 0,
            next: SYSTEM_INIT_OFF_MASK,
        };
        assert!(delta.touches_only_intended_bit());
        assert_eq!(delta.changed_bits(), SYSTEM_INIT_OFF_MASK);
    }

    #[test]
    fn backup_restore_value_is_the_previous_value() {
        let backup = Backup::new(
            identity(),
            BeeperConfigSnapshot {
                beeper_off_flags: 0,
                dshot_beacon_tone: 0,
                dshot_beacon_off_flags: 0,
            },
            DesiredDelta {
                field: "beeper_off_flags",
                mask: SYSTEM_INIT_OFF_MASK,
                previous: 0,
                next: SYSTEM_INIT_OFF_MASK,
            },
            RecoveryClass::AutomaticRollbackSupported,
        );
        assert_eq!(backup.restore_value(), 0);
        assert_eq!(backup.schema_version, BACKUP_SCHEMA_VERSION);
    }
}
