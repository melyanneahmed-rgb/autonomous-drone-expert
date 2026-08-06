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

/// The identity of a device, assembled from the four identification replies.
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
}

impl DeviceIdentity {
    /// Build an identity from the four decoded identification payloads.
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
        }
    }

    /// Compare two identities. Any difference is significant: after a reboot a different
    /// identity must abort, never be treated as the same unit.
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
    use ade_protocol_msp::{ApiVersion, BeeperConfigSnapshot, FcVariant, FcVersion};

    fn identity(board: [u8; 4]) -> DeviceIdentity {
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
            board_identifier: board,
        }
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
