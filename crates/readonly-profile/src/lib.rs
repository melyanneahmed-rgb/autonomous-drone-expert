#![forbid(unsafe_code)]

//! # `ade-readonly-profile` — read-profile selection without write authority
//!
//! M3 needs to learn how to *read* more than the exact M1 write target without silently making
//! those versions eligible for configuration changes. This crate makes that distinction a type
//! boundary. It describes only the already-proven read-only identity command sequence layouts;
//! it owns no command ids, frame bytes, transport, device handle, write approval or recovery path.

/// Stable identifier for a reviewed read-only identity layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadonlyIdentityProfileId {
    /// MSP protocol 0 / API 1.46 with the legacy three-byte `FC_VERSION` reply.
    BetaflightApi146Legacy,
    /// MSP protocol 0 / API 1.47 with the calendar-version `FC_VERSION` extension.
    BetaflightApi147CalendarExtended,
}

/// Typed `FC_VERSION` layout selected only after the API header is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FcVersionLayout {
    /// Exactly three version bytes.
    LegacyThreeByte,
    /// Three calendar-version bytes followed by a one-byte length and version-string bytes.
    CalendarTripletWithVersionString,
}

/// Typed board-info layout selector.
///
/// The pinned 1.46 and 1.47 records describe the same complete field sequence used by the current
/// stable identity model. A future incompatible layout must get a new variant rather than being
/// accepted as this one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardInfoLayout {
    /// Complete bounded board-info layout documented for API 1.46 and 1.47.
    Api146To147Complete,
}

/// Firmware-variant gate that must pass before a candidate becomes a selected profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariantGate {
    /// Require one exact four-byte flight-controller identifier.
    Exact([u8; 4]),
}

/// Write-authority statement carried by every read profile.
///
/// There deliberately is no write-enabled variant. A read profile can never be converted into a
/// `WriteApproval` or used as evidence that M1 write scope expanded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadProfileWriteAuthority {
    /// Read knowledge only; never authorises a hardware write.
    NeverAuthorizesWrites,
}

/// Candidate selected from the structurally valid `MSP_API_VERSION` tuple.
///
/// The firmware variant has not been accepted yet, so callers must apply [`accept_variant`]
/// before treating the candidate as a selected profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadonlyProfileCandidate {
    /// Stable profile identifier.
    pub id: ReadonlyIdentityProfileId,
    /// Exact protocol version.
    pub protocol_version: u8,
    /// Exact API major version.
    pub api_major: u8,
    /// Exact API minor version.
    pub api_minor: u8,
    /// Required four-byte firmware variant.
    pub variant_gate: VariantGate,
    /// `FC_VERSION` payload layout.
    pub fc_version_layout: FcVersionLayout,
    /// `BOARD_INFO` payload layout.
    pub board_info_layout: BoardInfoLayout,
    /// Permanent write-authority boundary for this read profile.
    pub write_authority: ReadProfileWriteAuthority,
}

/// Result of classifying a structurally valid API tuple for read-only identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadonlyApiProfileStatus {
    /// A reviewed read-only layout exists, but its firmware-variant gate still must pass.
    Candidate(ReadonlyProfileCandidate),
    /// The MSP protocol version itself has no reviewed read-only profile.
    UnsupportedProtocol,
    /// The protocol is known but this API version has no reviewed read-only profile.
    UnsupportedApi,
}

const BETAFIGHT_VARIANT: [u8; 4] = *b"BTFL";

/// Select a reviewed *read-only* identity profile candidate from an API tuple.
///
/// Recognising API 1.47 here is not product write support. The candidate permanently carries
/// [`ReadProfileWriteAuthority::NeverAuthorizesWrites`], still requires exact `BTFL` variant
/// confirmation, and does not modify `ade-facts::check_m1_api_scope`.
#[must_use]
pub const fn classify_readonly_api_profile(
    protocol_version: u8,
    api_major: u8,
    api_minor: u8,
) -> ReadonlyApiProfileStatus {
    if protocol_version != 0 {
        return ReadonlyApiProfileStatus::UnsupportedProtocol;
    }

    let candidate = match (api_major, api_minor) {
        (1, 46) => ReadonlyProfileCandidate {
            id: ReadonlyIdentityProfileId::BetaflightApi146Legacy,
            protocol_version: 0,
            api_major: 1,
            api_minor: 46,
            variant_gate: VariantGate::Exact(BETAFIGHT_VARIANT),
            fc_version_layout: FcVersionLayout::LegacyThreeByte,
            board_info_layout: BoardInfoLayout::Api146To147Complete,
            write_authority: ReadProfileWriteAuthority::NeverAuthorizesWrites,
        },
        (1, 47) => ReadonlyProfileCandidate {
            id: ReadonlyIdentityProfileId::BetaflightApi147CalendarExtended,
            protocol_version: 0,
            api_major: 1,
            api_minor: 47,
            variant_gate: VariantGate::Exact(BETAFIGHT_VARIANT),
            fc_version_layout: FcVersionLayout::CalendarTripletWithVersionString,
            board_info_layout: BoardInfoLayout::Api146To147Complete,
            write_authority: ReadProfileWriteAuthority::NeverAuthorizesWrites,
        },
        _ => return ReadonlyApiProfileStatus::UnsupportedApi,
    };

    ReadonlyApiProfileStatus::Candidate(candidate)
}

/// Whether the observed four-byte firmware variant completes a candidate's profile gate.
#[must_use]
pub const fn accept_variant(candidate: ReadonlyProfileCandidate, observed: [u8; 4]) -> bool {
    match candidate.variant_gate {
        VariantGate::Exact(expected) => expected == observed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ade_facts::{ApiScopeStatus, check_m1_api_scope};
    use ade_protocol_msp::ApiVersion;

    #[test]
    fn api_146_and_147_have_distinct_reviewed_read_candidates() {
        let ReadonlyApiProfileStatus::Candidate(api_146) =
            classify_readonly_api_profile(0, 1, 46)
        else {
            panic!("API 1.46 must have a reviewed read candidate");
        };
        assert_eq!(
            api_146.id,
            ReadonlyIdentityProfileId::BetaflightApi146Legacy
        );
        assert_eq!(api_146.fc_version_layout, FcVersionLayout::LegacyThreeByte);

        let ReadonlyApiProfileStatus::Candidate(api_147) =
            classify_readonly_api_profile(0, 1, 47)
        else {
            panic!("API 1.47 must have a reviewed read candidate");
        };
        assert_eq!(
            api_147.id,
            ReadonlyIdentityProfileId::BetaflightApi147CalendarExtended
        );
        assert_eq!(
            api_147.fc_version_layout,
            FcVersionLayout::CalendarTripletWithVersionString
        );
    }

    #[test]
    fn every_reviewed_read_candidate_is_structurally_write_blocked() {
        for api_minor in [46, 47] {
            let ReadonlyApiProfileStatus::Candidate(candidate) =
                classify_readonly_api_profile(0, 1, api_minor)
            else {
                panic!("reviewed API must have a candidate");
            };
            assert_eq!(
                candidate.write_authority,
                ReadProfileWriteAuthority::NeverAuthorizesWrites
            );
        }
    }

    #[test]
    fn candidate_requires_exact_betaflight_variant() {
        let ReadonlyApiProfileStatus::Candidate(candidate) =
            classify_readonly_api_profile(0, 1, 47)
        else {
            panic!("API 1.47 must have a candidate");
        };
        assert!(accept_variant(candidate, *b"BTFL"));
        assert!(!accept_variant(candidate, *b"INAV"));
        assert!(!accept_variant(candidate, *b"TEST"));
    }

    #[test]
    fn unknown_protocol_or_api_is_fail_closed() {
        assert_eq!(
            classify_readonly_api_profile(1, 1, 47),
            ReadonlyApiProfileStatus::UnsupportedProtocol
        );
        assert_eq!(
            classify_readonly_api_profile(0, 2, 47),
            ReadonlyApiProfileStatus::UnsupportedApi
        );
        assert_eq!(
            classify_readonly_api_profile(0, 1, 48),
            ReadonlyApiProfileStatus::UnsupportedApi
        );
    }

    #[test]
    fn known_api_147_read_profile_does_not_expand_m1_write_scope() {
        assert!(matches!(
            classify_readonly_api_profile(0, 1, 47),
            ReadonlyApiProfileStatus::Candidate(_)
        ));

        let write_scope = check_m1_api_scope(&ApiVersion {
            protocol_version: 0,
            api_major: 1,
            api_minor: 47,
        });
        assert_eq!(
            write_scope,
            ApiScopeStatus::Mismatch {
                field: "msp_api_version"
            }
        );
    }
}
