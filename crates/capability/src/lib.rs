#![forbid(unsafe_code)]

//! # `ade-capability` — the proposed M1 target (NOT hardware validated)
//!
//! The M1 slice targets one specific board/target/firmware combination. Every field here is
//! **proposed**: the board has not been purchased, connected or validated. The word
//! "supported" is deliberately absent — [`ValidationStatus`] has no such value.

/// How much confidence a capability claim carries. There is intentionally no "Supported"
/// or "Validated" value below real hardware observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationStatus {
    /// Proposed on paper; not connected, not observed, not validated.
    ProposedNotHardwareValidated,
}

impl ValidationStatus {
    /// The exact status string used across reports and documentation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ValidationStatus::ProposedNotHardwareValidated => "PROPOSED — NOT HARDWARE VALIDATED",
        }
    }
}

/// The single proposed target of the M1 beeper slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProposedTarget {
    /// Human board name.
    pub board: &'static str,
    /// Betaflight target identifier.
    pub betaflight_target: &'static str,
    /// Firmware family and version.
    pub firmware: &'static str,
    /// MSP API version.
    pub msp_api_version: &'static str,
    /// Validation status — always proposed in M1.
    pub status: ValidationStatus,
}

/// The proposed M1 target: SpeedyBee F405 V4 running Betaflight 4.5.5 (MSP API 1.46).
#[must_use]
pub const fn m1_proposed_target() -> ProposedTarget {
    ProposedTarget {
        board: "SpeedyBee F405 V4",
        betaflight_target: "SPEEDYBEEF405V4",
        firmware: "Betaflight 4.5.5",
        msp_api_version: "1.46",
        status: ValidationStatus::ProposedNotHardwareValidated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_target_is_proposed_and_never_claims_support() {
        let target = m1_proposed_target();
        assert_eq!(
            target.status,
            ValidationStatus::ProposedNotHardwareValidated
        );
        assert_eq!(target.status.as_str(), "PROPOSED — NOT HARDWARE VALIDATED");
        assert!(!target.status.as_str().to_lowercase().contains("supported"));
        assert_eq!(target.betaflight_target, "SPEEDYBEEF405V4");
    }
}
