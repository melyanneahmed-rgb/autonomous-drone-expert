#![forbid(unsafe_code)]

//! # `ade-capability-resolution` — facts-to-capability adapter
//!
//! This crate is the narrow M3 integration seam between authoritative identity facts and the
//! descriptive capability-pack model. It owns no protocol parser, transport, device handle,
//! storage, safety gate or write approval. The adapter can therefore classify read-only knowledge
//! without creating a second route to hardware authority.

use ade_capability::{
    CapabilityPackTrust, CapabilityPackWritePolicy, DescriptorError, FirmwareFamily,
    FirmwareVersion, ObservedFirmwareIdentity, ReadOnlyPackResolution,
    m3_review_only_betaflight_4_5_5_pack, resolve_read_only_pack,
};
use ade_facts::DeviceIdentity;

/// Failure to map an already-decoded device identity into a reviewed firmware family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityIdentityError {
    /// The firmware variant has no reviewed family mapping in this M3 slice.
    UnrecognizedFirmwareFamily,
}

/// Privacy-bounded view of authoritative facts for capability resolution.
///
/// The variant mapping is deliberately fail-closed. Only `BTFL`, which is already pinned by the
/// current product scope, maps to a firmware family. An unknown four-byte variant is not guessed
/// into Betaflight, INAV or any other family.
///
/// # Errors
/// Returns [`CapabilityIdentityError::UnrecognizedFirmwareFamily`] when the observed variant has
/// no reviewed mapping.
pub fn observed_capability_identity(
    identity: &DeviceIdentity,
) -> Result<ObservedFirmwareIdentity<'_>, CapabilityIdentityError> {
    let family = match &identity.variant.identifier {
        b"BTFL" => FirmwareFamily::Betaflight,
        _ => return Err(CapabilityIdentityError::UnrecognizedFirmwareFamily),
    };

    Ok(ObservedFirmwareIdentity {
        family,
        protocol_version: identity.api.protocol_version,
        api_major: identity.api.api_major,
        api_minor: identity.api.api_minor,
        version: FirmwareVersion::new(
            identity.version.major,
            identity.version.minor,
            identity.version.patch,
        ),
        target_name: &identity.target_name,
    })
}

/// Stable application-facing summary of review-only capability knowledge.
///
/// A matching descriptor carries its trust and write policy so callers cannot accidentally drop
/// the fact that the current pack is repository-review-only and write-blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewOnlyCapabilityStatus {
    /// The firmware family itself has no reviewed mapping.
    UnknownFirmwareFamily,
    /// The family is known but no reviewed descriptor matches API/version/target.
    NoMatch,
    /// Exactly one review-only descriptor matches.
    Match {
        /// Stable descriptor identifier.
        pack_id: &'static str,
        /// Current trust boundary.
        trust: CapabilityPackTrust,
        /// Current write boundary. In M3 this can only be `WritesBlocked`.
        write_policy: CapabilityPackWritePolicy,
    },
    /// More than one descriptor matched and no implicit choice is allowed.
    Ambiguous,
    /// Descriptor validation failed before matching.
    InvalidPack {
        /// Zero-based descriptor index.
        index: usize,
        /// Structural validation failure.
        reason: DescriptorError,
    },
}

/// Resolve the authoritative identity against the first repository-embedded M3 descriptor set.
///
/// This is a read-only knowledge operation. It intentionally does not call or replace the M1
/// write-scope check and cannot return a write approval.
#[must_use]
pub fn resolve_review_only_capability(identity: &DeviceIdentity) -> ReviewOnlyCapabilityStatus {
    let observed = match observed_capability_identity(identity) {
        Ok(observed) => observed,
        Err(CapabilityIdentityError::UnrecognizedFirmwareFamily) => {
            return ReviewOnlyCapabilityStatus::UnknownFirmwareFamily;
        }
    };

    let packs = [m3_review_only_betaflight_4_5_5_pack()];
    match resolve_read_only_pack(&observed, &packs) {
        ReadOnlyPackResolution::NoMatch => ReviewOnlyCapabilityStatus::NoMatch,
        ReadOnlyPackResolution::Match(pack) => ReviewOnlyCapabilityStatus::Match {
            pack_id: pack.pack_id,
            trust: pack.trust,
            write_policy: pack.write_policy,
        },
        ReadOnlyPackResolution::Ambiguous => ReviewOnlyCapabilityStatus::Ambiguous,
        ReadOnlyPackResolution::InvalidPack { index, reason } => {
            ReviewOnlyCapabilityStatus::InvalidPack { index, reason }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ade_facts::DeviceIdentity;
    use ade_protocol_msp::{ApiVersion, FcVariant, FcVersion};

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
            hardware_revision: 7,
            fc_type: 0,
            target_capabilities: 1,
            target_name: "SPEEDYBEEF405V4".to_string(),
            board_name: "SpeedyBee F405 V4".to_string(),
            manufacturer_id: "SPB".to_string(),
            mcu_type_id: 0x1b,
        }
    }

    #[test]
    fn exact_authoritative_identity_maps_to_review_only_write_blocked_knowledge() {
        assert_eq!(
            resolve_review_only_capability(&identity()),
            ReviewOnlyCapabilityStatus::Match {
                pack_id: "bf-4.5.5-api1.46-speedybeef405v4-review",
                trust: CapabilityPackTrust::ReviewOnlyEmbedded,
                write_policy: CapabilityPackWritePolicy::WritesBlocked,
            }
        );
    }

    #[test]
    fn api_or_target_drift_is_no_match_not_write_permission() {
        let mut api = identity();
        api.api.api_minor = 47;
        assert_eq!(
            resolve_review_only_capability(&api),
            ReviewOnlyCapabilityStatus::NoMatch
        );

        let mut target = identity();
        target.target_name = "SPEEDYBEEF405V5".to_string();
        assert_eq!(
            resolve_review_only_capability(&target),
            ReviewOnlyCapabilityStatus::NoMatch
        );
    }

    #[test]
    fn unknown_variant_is_not_guessed_into_a_firmware_family() {
        let mut unknown = identity();
        unknown.variant.identifier = *b"TEST";
        assert_eq!(
            observed_capability_identity(&unknown),
            Err(CapabilityIdentityError::UnrecognizedFirmwareFamily)
        );
        assert_eq!(
            resolve_review_only_capability(&unknown),
            ReviewOnlyCapabilityStatus::UnknownFirmwareFamily
        );
    }

    #[test]
    fn adapter_drops_unit_specific_and_unneeded_identity_fields() {
        let mut a = identity();
        let mut b = identity();
        a.board_identifier = *b"AAAA";
        b.board_identifier = *b"BBBB";
        a.hardware_revision = 1;
        b.hardware_revision = 999;
        a.board_name = "private-a".to_string();
        b.board_name = "private-b".to_string();
        a.manufacturer_id = "A".to_string();
        b.manufacturer_id = "B".to_string();

        assert_eq!(
            observed_capability_identity(&a),
            observed_capability_identity(&b)
        );
    }
}
