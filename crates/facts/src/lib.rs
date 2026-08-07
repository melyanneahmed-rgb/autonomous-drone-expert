#![forbid(unsafe_code)]

//! # `ade-facts` — identity and beeper facts with two-dimensional provenance
//!
//! Facts observed from a device (identity, beeper snapshot) carried together with their
//! provenance state (ADR-0008): **where** a fact came from ([`SourceState`]) is independent
//! of **what our implementation has done with it** ([`VerificationState`]). A mock exercise
//! is never hardware observation.

use ade_protocol_msp::{ApiVersion, BeeperConfigSnapshot, BoardInfo, FcVariant, FcVersion};

/// Where a fact came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceState {
    /// Read from a device (or a model of one) during this session.
    Observed,
    /// Recorded from a pinned upstream source.
    PinnedSourceRecorded,
}

/// What our implementation has done with a fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationState {
    /// Not reproduced by our code.
    NotReproduced,
    /// Exercised against a mock/replay — internal consistency only, not hardware evidence.
    MockExercised,
    /// Observed on real hardware. Never reachable in the M1 Mock/Replay slice.
    HardwareObserved,
}

/// The stable reconnection identity of a device, assembled from the four identification
/// replies.
///
/// This is a *composite* identity: after a reboot the device must present a byte-for-byte
/// identical value here, or the session aborts rather than assume it is the same unit. It is
/// built **only** from stable descriptors and deliberately excludes:
///
/// * volatile runtime state from `MSP_BOARD_INFO` — `configuration_state`,
///   `gyro_sample_rate_hz`, `configuration_problems`, and the SPI/I2C device counts — which
///   legitimately change across reboots and must not cause a false "different device";
/// * the board `signature` — a per-unit value. It is excluded both because it is not a
///   stable *type* descriptor and to avoid retaining a unit-unique identifier (privacy); it
///   is therefore never stored here, in a backup, or in a case record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceIdentity {
    /// MSP API version.
    pub api: ApiVersion,
    /// Flight-controller variant identifier (e.g. `BTFL`).
    pub variant: FcVariant,
    /// Firmware version.
    pub version: FcVersion,
    /// Four-byte board identifier from the board-info reply.
    pub board_identifier: [u8; 4],
    /// Hardware revision.
    pub hardware_revision: u16,
    /// FC type byte (0 = FC, 2 = FC with OSD chip).
    pub fc_type: u8,
    /// Target capability bitfield.
    pub target_capabilities: u8,
    /// Target name (e.g. `SPEEDYBEEF405V4`).
    pub target_name: String,
    /// Board name.
    pub board_name: String,
    /// Manufacturer identifier.
    pub manufacturer_id: String,
    /// MCU type identifier.
    pub mcu_type_id: u8,
}

impl DeviceIdentity {
    /// Build an identity from the four decoded identification payloads.
    ///
    /// Only the stable descriptors of `board` are retained; its volatile runtime fields and
    /// its per-unit `signature` are intentionally dropped (see the type documentation).
    #[must_use]
    pub fn from_parts(
        api: ApiVersion,
        variant: FcVariant,
        version: FcVersion,
        board: &BoardInfo,
    ) -> Self {
        Self {
            api,
            variant,
            version,
            board_identifier: board.board_identifier,
            hardware_revision: board.hardware_revision,
            fc_type: board.fc_type,
            target_capabilities: board.target_capabilities,
            target_name: board.target_name.clone(),
            board_name: board.board_name.clone(),
            manufacturer_id: board.manufacturer_id.clone(),
            mcu_type_id: board.mcu_type_id,
        }
    }

    /// Compare two identities. Any difference in a stable descriptor is significant: after a
    /// reboot a different identity must abort, never be treated as the same unit.
    #[must_use]
    pub fn compare(&self, other: &DeviceIdentity) -> IdentityMatch {
        if self == other {
            IdentityMatch::Same
        } else {
            IdentityMatch::Different
        }
    }
}

/// The result of comparing two identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityMatch {
    /// Byte-for-byte the same identity.
    Same,
    /// The identity changed — the connected device is not the one we started with.
    Different,
}

/// A beeper snapshot fact carrying its provenance state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeeperSnapshotFact {
    /// The observed beeper configuration.
    pub snapshot: BeeperConfigSnapshot,
    /// Where it came from.
    pub source: SourceState,
    /// What we have done with it.
    pub verification: VerificationState,
}

impl BeeperSnapshotFact {
    /// A snapshot observed during a session (source = observed).
    #[must_use]
    pub fn observed(snapshot: BeeperConfigSnapshot, verification: VerificationState) -> Self {
        Self {
            snapshot,
            source: SourceState::Observed,
            verification,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ade_protocol_msp::{ApiVersion, BeeperConfigSnapshot, BoardInfo, FcVariant, FcVersion};

    fn api() -> ApiVersion {
        ApiVersion {
            protocol_version: 0,
            api_major: 1,
            api_minor: 46,
        }
    }

    fn variant() -> FcVariant {
        FcVariant {
            identifier: *b"BTFL",
        }
    }

    fn version() -> FcVersion {
        FcVersion {
            major: 4,
            minor: 5,
            patch: 5,
        }
    }

    /// A complete board-info reply model whose stable fields are all fixed and whose volatile
    /// fields and signature can be varied independently by tests.
    fn board() -> BoardInfo {
        BoardInfo {
            board_identifier: *b"S405",
            hardware_revision: 7,
            fc_type: 0,
            target_capabilities: 0b0000_0001,
            target_name: "SPEEDYBEEF405V4".to_string(),
            board_name: "SpeedyBee F405 V4".to_string(),
            manufacturer_id: "SPB".to_string(),
            signature: [0u8; 32],
            mcu_type_id: 0x1B,
            configuration_state: 1,
            gyro_sample_rate_hz: 8000,
            configuration_problems: 0,
            spi_device_count: 3,
            i2c_device_count: 1,
        }
    }

    fn identity_from(board: &BoardInfo) -> DeviceIdentity {
        DeviceIdentity::from_parts(api(), variant(), version(), board)
    }

    fn identity(board_id: [u8; 4]) -> DeviceIdentity {
        let mut b = board();
        b.board_identifier = board_id;
        identity_from(&b)
    }

    #[test]
    fn identical_identities_match_and_different_ones_do_not() {
        let a = identity(*b"S405");
        let b = identity(*b"S405");
        let c = identity(*b"XXXX");
        assert_eq!(a.compare(&b), IdentityMatch::Same);
        assert_eq!(a.compare(&c), IdentityMatch::Different);
    }

    #[test]
    fn a_different_target_name_is_a_different_device() {
        let a = identity_from(&board());
        let mut other = board();
        other.target_name = "SOMEOTHERTARGET".to_string();
        assert_eq!(a.compare(&identity_from(&other)), IdentityMatch::Different);
    }

    #[test]
    fn a_different_manufacturer_is_a_different_device() {
        let a = identity_from(&board());
        let mut other = board();
        other.manufacturer_id = "XYZ".to_string();
        assert_eq!(a.compare(&identity_from(&other)), IdentityMatch::Different);
    }

    #[test]
    fn a_different_mcu_type_is_a_different_device() {
        let a = identity_from(&board());
        let mut other = board();
        other.mcu_type_id = 0x2C;
        assert_eq!(a.compare(&identity_from(&other)), IdentityMatch::Different);
    }

    #[test]
    fn volatile_configuration_problems_do_not_change_identity() {
        let a = identity_from(&board());
        let mut other = board();
        other.configuration_problems = 0xDEAD_BEEF;
        // Also vary the other volatile fields to prove none of them are part of identity.
        other.configuration_state = 9;
        other.gyro_sample_rate_hz = 3200;
        other.spi_device_count = 0;
        other.i2c_device_count = 0;
        assert_eq!(a.compare(&identity_from(&other)), IdentityMatch::Same);
    }

    #[test]
    fn the_signature_is_neither_stored_nor_compared() {
        // Two boards identical in every stable field but with different per-unit signatures
        // yield the same identity: the signature is not retained by DeviceIdentity at all.
        let a = identity_from(&board());
        let mut other = board();
        other.signature = [0xFFu8; 32];
        let b = identity_from(&other);
        assert_eq!(a.compare(&b), IdentityMatch::Same);
        assert_eq!(
            a, b,
            "identities are byte-for-byte equal; no signature is held"
        );
    }

    #[test]
    fn a_mock_exercised_fact_is_not_hardware_observed() {
        let fact = BeeperSnapshotFact::observed(
            BeeperConfigSnapshot {
                beeper_off_flags: 0,
                dshot_beacon_tone: 0,
                dshot_beacon_off_flags: 0,
            },
            VerificationState::MockExercised,
        );
        assert_eq!(fact.source, SourceState::Observed);
        assert_ne!(fact.verification, VerificationState::HardwareObserved);
    }
}
