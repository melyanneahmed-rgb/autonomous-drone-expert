#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def replace_once(path: str, old: str, new: str) -> None:
    file = ROOT / path
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one match in {path}, found {count}: {old[:120]!r}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")


execution_path = ROOT / "crates/execution/src/lib.rs"
if "ReadOnlyComplete(ReadonlyProfileIdentity)" not in execution_path.read_text(encoding="utf-8"):
    replace_once(
        "crates/execution/Cargo.toml",
        'ade-facts = { path = "../facts" }\n',
        'ade-facts = { path = "../facts" }\nade-readonly-profile = { path = "../readonly-profile" }\n',
    )

    replace_once(
        "crates/execution/src/lib.rs",
        '''use ade_facts::{ApiScopeStatus, DeviceIdentity, check_m1_api_scope};
use ade_protocol_msp::{
    ApiVersion, BeeperConfigSnapshot, CommandId, Correlator, Direction, FcVariant, FcVersion,
    Frame, MspError, ReplyClass, SetBeeperConfig, decode_frame, encode_frame,
};
''',
        '''use ade_facts::DeviceIdentity;
use ade_protocol_msp::{
    ApiVersion, BeeperConfigSnapshot, CommandId, Correlator, Direction, FcVariant, Frame, MspError,
    ReplyClass, SetBeeperConfig, decode_frame, encode_frame,
};
use ade_readonly_profile::{
    ReadProfileWriteAuthority, ReadonlyApiProfileStatus, ReadonlyFcVersion,
    ReadonlyIdentityProfileId, ReadonlyProfileCandidate, ReadonlyProfileDecodeError,
    accept_variant, classify_readonly_api_profile, decode_fc_version,
};
''',
    )

    replace_once(
        "crates/execution/src/lib.rs",
        '''    /// A structurally valid API reply is outside the proposed product scope.
    ApiScopeMismatch {
        /// Stable field label; never a raw payload or device value.
        field: &'static str,
    },
''',
        '''    /// A structurally valid API reply has no reviewed read-only profile.
    ApiScopeMismatch {
        /// Stable field label; never a raw payload or device value.
        field: &'static str,
    },
    /// A reviewed API profile was selected but its exact firmware-family gate failed.
    ReadProfileMismatch {
        /// Stable field label; never a raw payload or device value.
        field: &'static str,
    },
''',
    )

    old_progress = '''/// Typed progress from the Rust-owned read-only identification state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentificationProgress {
    /// The exact supported API path requires the next canonical empty-payload read.
    Pending,
    /// All four supported replies produced a complete device identity.
    Complete(DeviceIdentity),
    /// The API reply was structurally valid but outside the proposed product scope.
    ///
    /// No variant, firmware version, board information, or complete identity exists here.
    ApiScopeMismatch {
        /// The only parsed fact available at this point.
        api: ApiVersion,
        /// Stable scope field label.
        field: &'static str,
    },
}
'''
    new_progress = '''/// Stable identity facts from a reviewed read-only profile that is intentionally not write-eligible.
///
/// This type is separate from [`DeviceIdentity`]: callers cannot accidentally pass an API 1.47
/// read result into the existing M1 write-capable identity flow. Per-unit signature bytes and
/// volatile board state are deliberately not retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadonlyProfileIdentity {
    /// Stable reviewed read-profile identifier.
    pub profile_id: ReadonlyIdentityProfileId,
    /// Permanent read-profile write boundary.
    pub write_authority: ReadProfileWriteAuthority,
    /// Observed MSP API tuple.
    pub api: ApiVersion,
    /// Exact four-byte firmware variant.
    pub variant: FcVariant,
    /// Profile-specific firmware-version representation.
    pub version: ReadonlyFcVersion,
    /// Four-byte board identifier.
    pub board_identifier: [u8; 4],
    /// Hardware revision.
    pub hardware_revision: u16,
    /// FC type byte.
    pub fc_type: u8,
    /// Target capability bitfield.
    pub target_capabilities: u8,
    /// Stable target name.
    pub target_name: String,
    /// Stable board name.
    pub board_name: String,
    /// Stable manufacturer identifier.
    pub manufacturer_id: String,
    /// MCU type identifier.
    pub mcu_type_id: u8,
}

impl ReadonlyProfileIdentity {
    fn from_parts(
        candidate: ReadonlyProfileCandidate,
        api: ApiVersion,
        variant: FcVariant,
        version: ReadonlyFcVersion,
        board: &ade_protocol_msp::BoardInfo,
    ) -> Self {
        Self {
            profile_id: candidate.id,
            write_authority: candidate.write_authority,
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

    /// Human-facing firmware version for the bounded read-only result.
    #[must_use]
    pub fn fc_version_string(&self) -> String {
        match &self.version {
            ReadonlyFcVersion::Legacy(version) => {
                format!("{}.{}.{}", version.major, version.minor, version.patch)
            }
            ReadonlyFcVersion::CalendarExtended { version_string, .. } => version_string.clone(),
        }
    }
}

/// Typed progress from the Rust-owned read-only identification state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentificationProgress {
    /// A reviewed read profile requires the next canonical empty-payload read.
    Pending,
    /// The legacy API 1.46 path produced the existing M1 device identity.
    Complete(DeviceIdentity),
    /// A reviewed read-only profile outside M1 write eligibility completed safely.
    ReadOnlyComplete(ReadonlyProfileIdentity),
    /// The API reply was structurally valid but no reviewed read-only profile exists.
    ApiScopeMismatch {
        /// The only parsed fact available at this point.
        api: ApiVersion,
        /// Stable scope field label.
        field: &'static str,
    },
    /// A reviewed API profile existed but the exact firmware-variant gate failed.
    ReadProfileMismatch {
        /// The API fact established before the variant gate.
        api: ApiVersion,
        /// Stable mismatch field.
        field: &'static str,
    },
}
'''
    replace_once("crates/execution/src/lib.rs", old_progress, new_progress)

    replace_once(
        "crates/execution/src/lib.rs",
        '''pub struct ReadonlyIdentification {
    stage: IdentificationStage,
    request_pending: bool,
    correlator: Correlator,
    api: Option<ApiVersion>,
    variant: Option<FcVariant>,
    version: Option<FcVersion>,
}
''',
        '''pub struct ReadonlyIdentification {
    stage: IdentificationStage,
    request_pending: bool,
    correlator: Correlator,
    api: Option<ApiVersion>,
    read_profile: Option<ReadonlyProfileCandidate>,
    variant: Option<FcVariant>,
    version: Option<ReadonlyFcVersion>,
}
''',
    )
    replace_once(
        "crates/execution/src/lib.rs",
        '''            api: None,
            variant: None,
            version: None,
''',
        '''            api: None,
            read_profile: None,
            variant: None,
            version: None,
''',
    )

    old_stages = '''            IdentificationStage::ApiVersion => {
                let api = ApiVersion::from_reply(frame)?;
                match check_m1_api_scope(&api) {
                    ApiScopeStatus::InScope => {
                        self.api = Some(api);
                        self.stage = IdentificationStage::FcVariant;
                        Ok(IdentificationProgress::Pending)
                    }
                    ApiScopeStatus::Mismatch { field } => {
                        self.stage = IdentificationStage::Complete;
                        Ok(IdentificationProgress::ApiScopeMismatch { api, field })
                    }
                }
            }
            IdentificationStage::FcVariant => {
                self.variant = Some(FcVariant::from_reply(frame)?);
                self.stage = IdentificationStage::FcVersion;
                Ok(IdentificationProgress::Pending)
            }
            IdentificationStage::FcVersion => {
                self.version = Some(FcVersion::from_reply(frame)?);
                self.stage = IdentificationStage::BoardInfo;
                Ok(IdentificationProgress::Pending)
            }
            IdentificationStage::BoardInfo => {
                let board = ade_protocol_msp::BoardInfo::from_reply(frame)?;
                self.stage = IdentificationStage::Complete;
                Ok(IdentificationProgress::Complete(
                    DeviceIdentity::from_parts(
                        self.api
                            .take()
                            .ok_or(ExecError::NoIdentificationRequestPending)?,
                        self.variant
                            .take()
                            .ok_or(ExecError::NoIdentificationRequestPending)?,
                        self.version
                            .take()
                            .ok_or(ExecError::NoIdentificationRequestPending)?,
                        &board,
                    ),
                ))
            }
'''
    new_stages = '''            IdentificationStage::ApiVersion => {
                let api = ApiVersion::from_reply(frame)?;
                match classify_readonly_api_profile(
                    api.protocol_version,
                    api.api_major,
                    api.api_minor,
                ) {
                    ReadonlyApiProfileStatus::Candidate(candidate) => {
                        self.api = Some(api);
                        self.read_profile = Some(candidate);
                        self.stage = IdentificationStage::FcVariant;
                        Ok(IdentificationProgress::Pending)
                    }
                    ReadonlyApiProfileStatus::UnsupportedProtocol => {
                        self.stage = IdentificationStage::Complete;
                        Ok(IdentificationProgress::ApiScopeMismatch {
                            api,
                            field: "protocol_version",
                        })
                    }
                    ReadonlyApiProfileStatus::UnsupportedApi => {
                        self.stage = IdentificationStage::Complete;
                        Ok(IdentificationProgress::ApiScopeMismatch {
                            api,
                            field: "msp_api_version",
                        })
                    }
                }
            }
            IdentificationStage::FcVariant => {
                let variant = FcVariant::from_reply(frame)?;
                let candidate = self
                    .read_profile
                    .ok_or(ExecError::NoIdentificationRequestPending)?;
                if !accept_variant(candidate, variant.identifier) {
                    let api = self
                        .api
                        .take()
                        .ok_or(ExecError::NoIdentificationRequestPending)?;
                    self.read_profile = None;
                    self.stage = IdentificationStage::Complete;
                    return Ok(IdentificationProgress::ReadProfileMismatch {
                        api,
                        field: "fc_variant",
                    });
                }
                self.variant = Some(variant);
                self.stage = IdentificationStage::FcVersion;
                Ok(IdentificationProgress::Pending)
            }
            IdentificationStage::FcVersion => {
                let candidate = self
                    .read_profile
                    .ok_or(ExecError::NoIdentificationRequestPending)?;
                let variant = self
                    .variant
                    .as_ref()
                    .ok_or(ExecError::NoIdentificationRequestPending)?;
                let version = match decode_fc_version(candidate, variant.identifier, frame) {
                    Ok(version) => version,
                    Err(ReadonlyProfileDecodeError::VariantMismatch) => {
                        return Err(ExecError::ReadProfileMismatch {
                            field: "fc_variant",
                        });
                    }
                    Err(ReadonlyProfileDecodeError::Protocol(error)) => {
                        return Err(ExecError::Payload(error));
                    }
                };
                self.version = Some(version);
                self.stage = IdentificationStage::BoardInfo;
                Ok(IdentificationProgress::Pending)
            }
            IdentificationStage::BoardInfo => {
                let board = ade_protocol_msp::BoardInfo::from_reply(frame)?;
                let candidate = self
                    .read_profile
                    .take()
                    .ok_or(ExecError::NoIdentificationRequestPending)?;
                let api = self
                    .api
                    .take()
                    .ok_or(ExecError::NoIdentificationRequestPending)?;
                let variant = self
                    .variant
                    .take()
                    .ok_or(ExecError::NoIdentificationRequestPending)?;
                let version = self
                    .version
                    .take()
                    .ok_or(ExecError::NoIdentificationRequestPending)?;
                self.stage = IdentificationStage::Complete;
                match version {
                    ReadonlyFcVersion::Legacy(version) => Ok(IdentificationProgress::Complete(
                        DeviceIdentity::from_parts(api, variant, version, &board),
                    )),
                    version @ ReadonlyFcVersion::CalendarExtended { .. } => {
                        Ok(IdentificationProgress::ReadOnlyComplete(
                            ReadonlyProfileIdentity::from_parts(
                                candidate, api, variant, version, &board,
                            ),
                        ))
                    }
                }
            }
'''
    replace_once("crates/execution/src/lib.rs", old_stages, new_stages)

    replace_once(
        "crates/execution/src/lib.rs",
        '''            match identification.accept_response(&reply)? {
                IdentificationProgress::Pending => {}
                IdentificationProgress::Complete(identity) => return Ok(identity),
                IdentificationProgress::ApiScopeMismatch { field, .. } => {
                    return Err(ExecError::ApiScopeMismatch { field });
                }
            }
''',
        '''            match identification.accept_response(&reply)? {
                IdentificationProgress::Pending => {}
                IdentificationProgress::Complete(identity) => return Ok(identity),
                IdentificationProgress::ReadOnlyComplete(_) => {
                    return Err(ExecError::ApiScopeMismatch {
                        field: "msp_api_version",
                    });
                }
                IdentificationProgress::ApiScopeMismatch { field, .. } => {
                    return Err(ExecError::ApiScopeMismatch { field });
                }
                IdentificationProgress::ReadProfileMismatch { field, .. } => {
                    return Err(ExecError::ReadProfileMismatch { field });
                }
            }
''',
    )

    replace_once(
        "crates/execution/src/lib.rs",
        '''            match incremental.accept_response(&reply).unwrap() {
                IdentificationProgress::Pending => {}
                IdentificationProgress::Complete(identity) => break identity,
                IdentificationProgress::ApiScopeMismatch { .. } => {
                    panic!("the supported mock API cannot be rejected")
                }
            }
''',
        '''            match incremental.accept_response(&reply).unwrap() {
                IdentificationProgress::Pending => {}
                IdentificationProgress::Complete(identity) => break identity,
                IdentificationProgress::ReadOnlyComplete(_)
                | IdentificationProgress::ApiScopeMismatch { .. }
                | IdentificationProgress::ReadProfileMismatch { .. } => {
                    panic!("the supported legacy mock API cannot leave the legacy identity path")
                }
            }
''',
    )

    replace_once(
        "crates/execution/src/lib.rs",
        '''        for (payload, field) in [
            ([0, 1, 45], "msp_api_version"),
            ([0, 1, 47], "msp_api_version"),
            ([0, 2, 46], "msp_api_version"),
            ([1, 1, 46], "protocol_version"),
        ] {
''',
        '''        for (payload, field) in [
            ([0, 1, 45], "msp_api_version"),
            ([0, 1, 48], "msp_api_version"),
            ([0, 2, 46], "msp_api_version"),
            ([1, 1, 46], "protocol_version"),
        ] {
''',
    )

    test_path = ROOT / "crates/execution/tests/api147_readonly_identification.rs"
    test_path.parent.mkdir(parents=True, exist_ok=True)
    test_path.write_text(r'''use ade_execution::{IdentificationProgress, IdentificationStage, ReadonlyIdentification};
use ade_facts::{ApiScopeStatus, check_m1_api_scope};
use ade_protocol_msp::{
    ApiVersion, CommandId, Direction, Frame, SIGNATURE_LENGTH, decode_frame, encode_frame,
};
use ade_readonly_profile::{
    ReadProfileWriteAuthority, ReadonlyFcVersion, ReadonlyIdentityProfileId,
};
use ade_session::SessionState;

fn reply(command: CommandId, payload: &[u8]) -> Frame {
    let bytes = encode_frame(Direction::Reply, command, payload).expect("fixture encode");
    decode_frame(&bytes).expect("fixture decode")
}

fn assert_next(id: &mut ReadonlyIdentification, expected: CommandId) {
    let request = id.next_request().expect("next read");
    assert_eq!(request.command(), expected);
    let frame = decode_frame(request.bytes()).expect("request decode");
    assert_eq!(frame.direction, Direction::Request);
    assert_eq!(frame.payload_len(), 0);
}

fn board_payload(target: &str) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(b"F405");
    payload.extend_from_slice(&7u16.to_le_bytes());
    payload.extend_from_slice(&[0, 1]);
    for value in [target, "SpeedyBee F405", "SPB"] {
        payload.push(u8::try_from(value.len()).expect("bounded fixture"));
        payload.extend_from_slice(value.as_bytes());
    }
    payload.extend_from_slice(&[0xA5; SIGNATURE_LENGTH]);
    payload.extend_from_slice(&[0x1b, 0]);
    payload.extend_from_slice(&8000u16.to_le_bytes());
    payload.extend_from_slice(&0u32.to_le_bytes());
    payload.extend_from_slice(&[2, 1]);
    payload
}

#[test]
fn api_147_completes_four_read_only_identity_reads_without_write_eligibility() {
    let mut id = ReadonlyIdentification::new(SessionState::Identifying).expect("identity");
    assert_next(&mut id, CommandId::ApiVersion);
    assert_eq!(
        id.accept_response(&reply(CommandId::ApiVersion, &[0, 1, 47])),
        Ok(IdentificationProgress::Pending)
    );
    assert_next(&mut id, CommandId::FcVariant);
    assert_eq!(
        id.accept_response(&reply(CommandId::FcVariant, b"BTFL")),
        Ok(IdentificationProgress::Pending)
    );
    assert_next(&mut id, CommandId::FcVersion);
    let mut version_payload = vec![25, 12, 1, 9];
    version_payload.extend_from_slice(b"2025.12.1");
    assert_eq!(
        id.accept_response(&reply(CommandId::FcVersion, &version_payload)),
        Ok(IdentificationProgress::Pending)
    );
    assert_next(&mut id, CommandId::BoardInfo);
    let completed = id
        .accept_response(&reply(CommandId::BoardInfo, &board_payload("SPEEDYBEEF405V5")))
        .expect("read-only identity completion");
    let IdentificationProgress::ReadOnlyComplete(identity) = completed else {
        panic!("API 1.47 must finish as read-only profile identity");
    };
    assert_eq!(
        identity.profile_id,
        ReadonlyIdentityProfileId::BetaflightApi147CalendarExtended
    );
    assert_eq!(
        identity.write_authority,
        ReadProfileWriteAuthority::NeverAuthorizesWrites
    );
    assert_eq!(identity.api.api_minor, 47);
    assert_eq!(identity.variant.identifier, *b"BTFL");
    assert_eq!(identity.target_name, "SPEEDYBEEF405V5");
    assert_eq!(identity.fc_version_string(), "2025.12.1");
    assert_eq!(
        identity.version,
        ReadonlyFcVersion::CalendarExtended {
            calendar_version: [25, 12, 1],
            version_string: "2025.12.1".to_owned(),
        }
    );
    assert_eq!(
        check_m1_api_scope(&identity.api),
        ApiScopeStatus::Mismatch {
            field: "msp_api_version"
        }
    );
    assert!(id.is_complete());
}

#[test]
fn api_147_wrong_variant_stops_before_fc_version_read() {
    let mut id = ReadonlyIdentification::new(SessionState::Identifying).expect("identity");
    assert_next(&mut id, CommandId::ApiVersion);
    assert_eq!(
        id.accept_response(&reply(CommandId::ApiVersion, &[0, 1, 47])),
        Ok(IdentificationProgress::Pending)
    );
    assert_next(&mut id, CommandId::FcVariant);
    assert_eq!(
        id.accept_response(&reply(CommandId::FcVariant, b"INAV")),
        Ok(IdentificationProgress::ReadProfileMismatch {
            api: ApiVersion {
                protocol_version: 0,
                api_major: 1,
                api_minor: 47,
            },
            field: "fc_variant",
        })
    );
    assert!(id.is_complete());
    assert!(id.next_request().is_err());
}

#[test]
fn unknown_api_still_stops_after_the_single_api_read() {
    let mut id = ReadonlyIdentification::new(SessionState::Identifying).expect("identity");
    assert_next(&mut id, CommandId::ApiVersion);
    assert_eq!(
        id.accept_response(&reply(CommandId::ApiVersion, &[0, 1, 48])),
        Ok(IdentificationProgress::ApiScopeMismatch {
            api: ApiVersion {
                protocol_version: 0,
                api_major: 1,
                api_minor: 48,
            },
            field: "msp_api_version",
        })
    );
    assert!(id.is_complete());
}

#[test]
fn api_146_preserves_existing_device_identity_completion_type() {
    let mut id = ReadonlyIdentification::new(SessionState::Identifying).expect("identity");
    for (command, payload) in [
        (CommandId::ApiVersion, vec![0, 1, 46]),
        (CommandId::FcVariant, b"BTFL".to_vec()),
        (CommandId::FcVersion, vec![4, 5, 5]),
    ] {
        assert_next(&mut id, command);
        assert_eq!(
            id.accept_response(&reply(command, &payload)),
            Ok(IdentificationProgress::Pending)
        );
    }
    assert_next(&mut id, CommandId::BoardInfo);
    assert!(matches!(
        id.accept_response(&reply(
            CommandId::BoardInfo,
            &board_payload("SPEEDYBEEF405V4")
        )),
        Ok(IdentificationProgress::Complete(_))
    ));
}
''', encoding="utf-8")

bridge_path = ROOT / "crates/web-readonly-serial-wasm-bridge/src/lib.rs"
bridge_text = bridge_path.read_text(encoding="utf-8")
if "ReadOnlyComplete(ReadonlyProfileIdentity)" not in bridge_text:
    replace_once(
        "crates/web-readonly-serial-wasm-bridge/src/lib.rs",
        '''use ade_execution::{
    ExecError, IdentificationProgress, IdentificationRequest, IdentificationStage,
    ReadonlyIdentification,
};
''',
        '''use ade_execution::{
    ExecError, IdentificationProgress, IdentificationRequest, IdentificationStage,
    ReadonlyIdentification, ReadonlyProfileIdentity,
};
''',
    )
    replace_once(
        "crates/web-readonly-serial-wasm-bridge/src/lib.rs",
        '''    ApiScopeMismatch {
        api: ApiVersion,
        field: &'static str,
    },
    Failed {
''',
        '''    ApiScopeMismatch {
        api: ApiVersion,
        field: &'static str,
    },
    ReadOnlyComplete(ReadonlyProfileIdentity),
    ReadProfileMismatch {
        api: ApiVersion,
        field: &'static str,
    },
    Failed {
''',
    )
    replace_once(
        "crates/web-readonly-serial-wasm-bridge/src/lib.rs",
        '''            Ok(IdentificationProgress::Pending) => {
                self.push_identity_trace("IDENTITY_STAGE_OK", stage, command, None);
                self.next_exchange().map(Some)
            }
            Ok(IdentificationProgress::ApiScopeMismatch { api, field }) => {
''',
        '''            Ok(IdentificationProgress::ReadOnlyComplete(identity)) => {
                self.push_identity_trace("IDENTITY_STAGE_OK", stage, command, None);
                self.outcome = Some(FinalOutcome::ReadOnlyComplete(identity));
                self.start_close().map(Some)
            }
            Ok(IdentificationProgress::Pending) => {
                self.push_identity_trace("IDENTITY_STAGE_OK", stage, command, None);
                self.next_exchange().map(Some)
            }
            Ok(IdentificationProgress::ApiScopeMismatch { api, field }) => {
''',
    )
    replace_once(
        "crates/web-readonly-serial-wasm-bridge/src/lib.rs",
        '''                self.outcome = Some(FinalOutcome::ApiScopeMismatch { api, field });
                self.start_close().map(Some)
            }
            Err(error) => {
''',
        '''                self.outcome = Some(FinalOutcome::ApiScopeMismatch { api, field });
                self.start_close().map(Some)
            }
            Ok(IdentificationProgress::ReadProfileMismatch { api, field }) => {
                self.push_identity_trace("IDENTITY_STAGE_OK", stage, command, None);
                self.outcome = Some(FinalOutcome::ReadProfileMismatch { api, field });
                self.start_close().map(Some)
            }
            Err(error) => {
''',
    )
    replace_once(
        "crates/web-readonly-serial-wasm-bridge/src/lib.rs",
        '''            FinalOutcome::ApiScopeMismatch { .. } | FinalOutcome::Failed { .. } => None,
        }
    }
}
''',
        '''            FinalOutcome::ApiScopeMismatch { .. }
            | FinalOutcome::ReadOnlyComplete(_)
            | FinalOutcome::ReadProfileMismatch { .. }
            | FinalOutcome::Failed { .. } => None,
        }
    }

    fn read_only_identity(&self) -> Option<&ReadonlyProfileIdentity> {
        match self.outcome.as_ref()? {
            FinalOutcome::ReadOnlyComplete(identity) => Some(identity),
            _ => None,
        }
    }
}
''',
    )
    replace_once(
        "crates/web-readonly-serial-wasm-bridge/src/lib.rs",
        '''            Some(FinalOutcome::ApiScopeMismatch { .. }) if self.phase == Phase::Complete => {
                "api-unsupported"
            }
            Some(FinalOutcome::Failed { .. }) if self.phase == Phase::Complete => "failed",
''',
        '''            Some(FinalOutcome::ApiScopeMismatch { .. }) if self.phase == Phase::Complete => {
                "api-unsupported"
            }
            Some(FinalOutcome::ReadOnlyComplete(_)) if self.phase == Phase::Complete => {
                "read-only-complete"
            }
            Some(FinalOutcome::ReadProfileMismatch { .. }) if self.phase == Phase::Complete => {
                "read-profile-unsupported"
            }
            Some(FinalOutcome::Failed { .. }) if self.phase == Phase::Complete => "failed",
''',
    )
    replace_once(
        "crates/web-readonly-serial-wasm-bridge/src/lib.rs",
        '''                FinalOutcome::ScopeMismatch { field, .. }
                | FinalOutcome::ApiScopeMismatch { field, .. },
''',
        '''                FinalOutcome::ScopeMismatch { field, .. }
                | FinalOutcome::ApiScopeMismatch { field, .. }
                | FinalOutcome::ReadProfileMismatch { field, .. },
''',
    )
    replace_once(
        "crates/web-readonly-serial-wasm-bridge/src/lib.rs",
        '''            Some(FinalOutcome::ApiScopeMismatch { api, .. }) => {
                Some(format!("{}.{}", api.api_major, api.api_minor))
            }
            _ => self
                .identity()
                .map(|identity| format!("{}.{}", identity.api.api_major, identity.api.api_minor)),
''',
        '''            Some(
                FinalOutcome::ApiScopeMismatch { api, .. }
                | FinalOutcome::ReadProfileMismatch { api, .. },
            ) => Some(format!("{}.{}", api.api_major, api.api_minor)),
            Some(FinalOutcome::ReadOnlyComplete(identity)) => Some(format!(
                "{}.{}",
                identity.api.api_major, identity.api.api_minor
            )),
            _ => self
                .identity()
                .map(|identity| format!("{}.{}", identity.api.api_major, identity.api.api_minor)),
''',
    )
    replace_once(
        "crates/web-readonly-serial-wasm-bridge/src/lib.rs",
        '''    pub fn fc_variant(&self) -> Option<String> {
        self.identity()
            .map(|identity| identity.variant.identifier_string())
    }
''',
        '''    pub fn fc_variant(&self) -> Option<String> {
        self.read_only_identity()
            .map(|identity| identity.variant.identifier_string())
            .or_else(|| {
                self.identity()
                    .map(|identity| identity.variant.identifier_string())
            })
    }
''',
    )
    replace_once(
        "crates/web-readonly-serial-wasm-bridge/src/lib.rs",
        '''    pub fn fc_version(&self) -> Option<String> {
        self.identity().map(|identity| {
            format!(
                "{}.{}.{}",
                identity.version.major, identity.version.minor, identity.version.patch
            )
        })
    }
''',
        '''    pub fn fc_version(&self) -> Option<String> {
        self.read_only_identity()
            .map(ReadonlyProfileIdentity::fc_version_string)
            .or_else(|| {
                self.identity().map(|identity| {
                    format!(
                        "{}.{}.{}",
                        identity.version.major, identity.version.minor, identity.version.patch
                    )
                })
            })
    }
''',
    )
    replace_once(
        "crates/web-readonly-serial-wasm-bridge/src/lib.rs",
        '''    pub fn target_name(&self) -> Option<String> {
        self.identity().map(|identity| identity.target_name.clone())
    }
''',
        '''    pub fn target_name(&self) -> Option<String> {
        self.read_only_identity()
            .map(|identity| identity.target_name.clone())
            .or_else(|| self.identity().map(|identity| identity.target_name.clone()))
    }
''',
    )
    replace_once(
        "crates/web-readonly-serial-wasm-bridge/src/lib.rs",
        '''            ([0, 1, 45], "1.45", "msp_api_version"),
            ([0, 1, 47], "1.47", "msp_api_version"),
            ([0, 2, 46], "2.46", "msp_api_version"),
''',
        '''            ([0, 1, 45], "1.45", "msp_api_version"),
            ([0, 1, 48], "1.48", "msp_api_version"),
            ([0, 2, 46], "2.46", "msp_api_version"),
''',
    )

    marker = '''    #[test]
    fn refused_identification_contexts_cannot_create_a_web_serial_directive() {
'''
    addition = '''    #[test]
    fn reviewed_api_147_betaflight_runs_all_four_reads_and_returns_read_only_identity() {
        let mut bridge = WasmReadonlySerialDiscovery::create().unwrap();
        let open = bridge.begin_open().unwrap();
        let api = bridge.accept_open_ok(&open.request_id).unwrap();
        let variant = feed_reply(
            &mut bridge,
            &api,
            Direction::Reply,
            CommandId::ApiVersion,
            &[0, 1, 47],
        );
        let version = feed_reply(
            &mut bridge,
            &variant,
            Direction::Reply,
            CommandId::FcVariant,
            b"BTFL",
        );
        let mut version_payload = vec![25, 12, 1, 9];
        version_payload.extend_from_slice(b"2025.12.1");
        let board = feed_reply(
            &mut bridge,
            &version,
            Direction::Reply,
            CommandId::FcVersion,
            &version_payload,
        );
        let close = feed_reply(
            &mut bridge,
            &board,
            Direction::Reply,
            CommandId::BoardInfo,
            &valid_board_payload(),
        );
        assert_eq!(close.kind, "close");
        assert_eq!(bridge.api_version().as_deref(), Some("1.47"));
        assert_eq!(bridge.fc_variant().as_deref(), Some("BTFL"));
        assert_eq!(bridge.fc_version().as_deref(), Some("2025.12.1"));
        assert_eq!(bridge.target_name().as_deref(), Some("SPEEDYBEEF405V4"));
        assert!(bridge.scope_mismatch_field().is_none());
        bridge.accept_close(&close.request_id, None).unwrap();
        assert_eq!(bridge.outcome_kind(), "read-only-complete");
        assert!(!bridge.hardware_observed());
        assert_eq!(
            bridge
                .trace_events
                .iter()
                .filter(|event| event.event == "DIRECTIVE")
                .count(),
            4,
        );
    }

    #[test]
    fn reviewed_api_147_wrong_variant_stops_before_fc_version() {
        let mut bridge = WasmReadonlySerialDiscovery::create().unwrap();
        let open = bridge.begin_open().unwrap();
        let api = bridge.accept_open_ok(&open.request_id).unwrap();
        let variant = feed_reply(
            &mut bridge,
            &api,
            Direction::Reply,
            CommandId::ApiVersion,
            &[0, 1, 47],
        );
        let close = feed_reply(
            &mut bridge,
            &variant,
            Direction::Reply,
            CommandId::FcVariant,
            b"INAV",
        );
        assert_eq!(close.kind, "close");
        assert_eq!(bridge.api_version().as_deref(), Some("1.47"));
        assert_eq!(bridge.scope_mismatch_field().as_deref(), Some("fc_variant"));
        assert!(bridge.fc_variant().is_none());
        bridge.accept_close(&close.request_id, None).unwrap();
        assert_eq!(bridge.outcome_kind(), "read-profile-unsupported");
        assert!(!bridge.hardware_observed());
        assert_eq!(
            bridge
                .trace_events
                .iter()
                .filter(|event| event.event == "DIRECTIVE")
                .count(),
            2,
        );
    }

'''
    replace_once(
        "crates/web-readonly-serial-wasm-bridge/src/lib.rs",
        marker,
        addition + marker,
    )

# Web result types and bounded UI states.
connection_types = ROOT / "web/src/connection/readonly-fc-connection.d.mts"
if '"read-only-complete"' not in connection_types.read_text(encoding="utf-8"):
    replace_once(
        "web/src/connection/readonly-fc-connection.d.mts",
        '''    | "scope-mismatch"
    | "api-unsupported"
    | "failed"
''',
        '''    | "scope-mismatch"
    | "read-only-complete"
    | "read-profile-unsupported"
    | "api-unsupported"
    | "failed"
''',
    )

app = ROOT / "web/src/App.tsx"
if '"read-only-complete"' not in app.read_text(encoding="utf-8"):
    replace_once(
        "web/src/App.tsx",
        '''  | "read-complete"
  | "scope-mismatch"
  | "api-unsupported"
''',
        '''  | "read-complete"
  | "scope-mismatch"
  | "read-only-complete"
  | "read-profile-unsupported"
  | "api-unsupported"
''',
    )
    replace_once(
        "web/src/App.tsx",
        '''  "read-complete": "اكتملت قراءة الهوية ضمن النطاق المقترح",
  "scope-mismatch": "اكتملت القراءة — الهوية خارج النطاق المقترح",
  "api-unsupported": "إصدار واجهة وحدة التحكم غير مدعوم حاليًا",
''',
        '''  "read-complete": "اكتملت قراءة الهوية ضمن النطاق المقترح",
  "scope-mismatch": "اكتملت القراءة — الهوية خارج النطاق المقترح",
  "read-only-complete": "اكتملت قراءة الهوية — هذا الإصدار مدعوم للقراءة فقط",
  "read-profile-unsupported": "نوع وحدة التحكم غير مدعوم ضمن ملفات القراءة الحالية",
  "api-unsupported": "إصدار واجهة وحدة التحكم غير مدعوم حاليًا",
''',
    )
    replace_once(
        "web/src/App.tsx",
        '''      } else if (result.outcome === "scope-mismatch") {
        setConnection({ phase: "scope-mismatch", result });
      } else if (result.outcome === "api-unsupported") {
''',
        '''      } else if (result.outcome === "scope-mismatch") {
        setConnection({ phase: "scope-mismatch", result });
      } else if (result.outcome === "read-only-complete") {
        setConnection({ phase: "read-only-complete", result });
      } else if (result.outcome === "read-profile-unsupported") {
        setConnection({ phase: "read-profile-unsupported", result });
      } else if (result.outcome === "api-unsupported") {
''',
    )
    replace_once(
        "web/src/App.tsx",
        '''              {connection.result && connection.phase !== "api-unsupported" && (
''',
        '''              {connection.result &&
                connection.phase !== "api-unsupported" &&
                connection.phase !== "read-profile-unsupported" && (
''',
    )

# Static boundary test must recognize the two new bounded UI states.
static_test = ROOT / "web/tests/webapp-readonly-fc-connection.test.mjs"
if '"read-only-complete"' not in static_test.read_text(encoding="utf-8"):
    replace_once(
        "web/tests/webapp-readonly-fc-connection.test.mjs",
        '''    "read-complete",
    "scope-mismatch",
    "api-unsupported",
''',
        '''    "read-complete",
    "scope-mismatch",
    "read-only-complete",
    "read-profile-unsupported",
    "api-unsupported",
''',
    )
    replace_once(
        "web/tests/webapp-readonly-fc-connection.test.mjs",
        '''  assert.match(app, /"api-unsupported": "إصدار واجهة وحدة التحكم غير مدعوم حاليًا"/);
  assert.match(app, /connection\.phase !== "api-unsupported"/);
''',
        '''  assert.match(app, /"read-only-complete": "اكتملت قراءة الهوية — هذا الإصدار مدعوم للقراءة فقط"/);
  assert.match(app, /"api-unsupported": "إصدار واجهة وحدة التحكم غير مدعوم حاليًا"/);
  assert.match(app, /connection\.phase !== "api-unsupported"/);
  assert.match(app, /connection\.phase !== "read-profile-unsupported"/);
''',
    )

# Low-level real-browser fixture: 1.47 is reviewed and no longer belongs to the early API-stop matrix.
low = ROOT / "web/tests/browser/webserial-readonly-smoke.mjs"
low_text = low.read_text(encoding="utf-8")
if 'const API147_READONLY_REPLIES' not in low_text:
    replace_once(
        "web/tests/browser/webserial-readonly-smoke.mjs",
        '''const API_SCOPE_REPLIES = [
  ["lower-minor", [36, 77, 62, 3, 1, 0, 1, 45, 46], "1.45", "normal"],
  ["higher-minor", [36, 77, 62, 3, 1, 0, 1, 47, 44], "1.47", "split-frame"],
  ["incompatible-major", [36, 77, 62, 3, 1, 0, 2, 46, 46], "2.46", "whole-frame"],
''',
        '''const API_SCOPE_REPLIES = [
  ["lower-minor", [36, 77, 62, 3, 1, 0, 1, 45, 46], "1.45", "normal"],
  ["higher-minor", [36, 77, 62, 3, 1, 0, 1, 48, 19], "1.48", "split-frame"],
  ["incompatible-major", [36, 77, 62, 3, 1, 0, 2, 46, 46], "2.46", "whole-frame"],
''',
    )
    # Build API 1.47 fixtures in test code to avoid importing protocol logic into production JS.
    replace_once(
        "web/tests/browser/webserial-readonly-smoke.mjs",
        '''const FULL_SCOPE_MISMATCH_REPLIES = [
''',
        '''function testReply(command, payload) {
  let checksum = payload.length ^ command;
  for (const byte of payload) checksum ^= byte;
  return Uint8Array.from([36, 77, 62, payload.length, command, ...payload, checksum]);
}
const API147_READONLY_REPLIES = [
  testReply(1, [0, 1, 47]),
  testReply(2, [...new TextEncoder().encode("BTFL")]),
  testReply(3, [25, 12, 1, 9, ...new TextEncoder().encode("2025.12.1")]),
  IN_SCOPE_REPLIES[3],
];
const FULL_SCOPE_MISMATCH_REPLIES = [
''',
    )
    marker = '''async function scenarioIHostFailureOriginsAndRetry() {
'''
    addition = '''async function scenarioH2Api147ReadOnly() {
  mark("H2-api147-read-only");
  const run = await runDiscovery(API147_READONLY_REPLIES);
  assert(run.result.outcome === "read-only-complete", "API 1.47 completes read-only identity");
  assert(run.result.apiVersion === "1.47", "API 1.47 is retained");
  assert(run.result.fcVariant === "BTFL", "API 1.47 exact variant retained");
  assert(run.result.fcVersion === "2025.12.1", "API 1.47 strict version string retained");
  assert(run.result.targetName === "SPEEDYBEEF405V4", "API 1.47 target retained");
  assert(run.result.scopeMismatchField === undefined, "read-only completion is not API unsupported");
  assert(run.result.hardwareObserved === false, "read-only completion is not hardware validation");
  assert(run.port.writes.length === 4, "API 1.47 uses exactly four empty reads");
  EXPECTED_REQUESTS.forEach((expected, index) =>
    assert(equalBytes(run.port.writes[index], expected), `API 1.47 request order ${index}`),
  );
}

'''
    replace_once("web/tests/browser/webserial-readonly-smoke.mjs", marker, addition + marker)
    # Find scenario execution footer and insert call after H.
    replace_once(
        "web/tests/browser/webserial-readonly-smoke.mjs",
        '''  await scenarioHScopeMismatch();
  await scenarioIHostFailureOriginsAndRetry();
''',
        '''  await scenarioHScopeMismatch();
  await scenarioH2Api147ReadOnly();
  await scenarioIHostFailureOriginsAndRetry();
''',
    )

# Production browser fixture/result: replace the old early 1.47 stop with a four-read read-only success.
prod = ROOT / "web/tests/webapp-readonly-fc-browser-smoke.mjs"
prod_text = prod.read_text(encoding="utf-8")
if '"api147-read-only"' not in prod_text:
    replace_once(
        "web/tests/webapp-readonly-fc-browser-smoke.mjs",
        '''  const boardPayloadWithTrailingByte = [...inScopeReplies[3].slice(5, -1), 0];
''',
        '''  const testReply = (command, payload) => {
    let checksum = payload.length ^ command;
    for (const byte of payload) checksum ^= byte;
    return Uint8Array.from([36, 77, 62, payload.length, command, ...payload, checksum]);
  };
  const api147Replies = [
    testReply(1, [0, 1, 47]),
    testReply(2, [...new TextEncoder().encode("BTFL")]),
    testReply(3, [25, 12, 1, 9, ...new TextEncoder().encode("2025.12.1")]),
    inScopeReplies[3],
  ];
  const boardPayloadWithTrailingByte = [...inScopeReplies[3].slice(5, -1), 0];
''',
    )
    replace_once(
        "web/tests/webapp-readonly-fc-browser-smoke.mjs",
        '''  const replies = scenario === "api-unsupported-147" || scenario === "fragmented-api-version"
    ? [Uint8Array.from([36, 77, 62, 3, 1, 0, 1, 47, 44])]
''',
        '''  const replies = scenario === "api147-read-only"
    ? api147Replies
    : scenario === "fragmented-api-version"
      ? [testReply(1, [0, 1, 48])]
''',
    )
    replace_once(
        "web/tests/webapp-readonly-fc-browser-smoke.mjs",
        '''    "read-complete",
    "scope-mismatch",
    "api-unsupported",
''',
        '''    "read-complete",
    "scope-mismatch",
    "read-only-complete",
    "read-profile-unsupported",
    "api-unsupported",
''',
    )
    old_api147 = '''  const api147 = await runScenario(
    browser,
    url,
    "api-unsupported-147",
    "api-unsupported",
    2,
  );
  if (
    JSON.stringify(api147.writes) !== JSON.stringify([expectedRequests[0], expectedRequests[0]]) ||
    Object.keys(api147.fields).length !== 0 ||
    api147.openCount !== 2 || api147.closeCount !== 2 ||
    api147.text.includes("1.47") || api147.text.includes("msp_api_version")
  ) throw new Error(`PRODUCTION_API_147_GATE_FAILED:${JSON.stringify(api147)}`);
'''
    new_api147 = '''  const api147 = await runScenario(
    browser,
    url,
    "api147-read-only",
    "read-only-complete",
    2,
  );
  if (
    JSON.stringify(api147.writes) !== JSON.stringify([
      ...expectedRequests,
      ...expectedRequests,
    ]) ||
    api147.fields.apiVersion !== "1.47" ||
    api147.fields.fcVariant !== "BTFL" ||
    api147.fields.fcVersion !== "2025.12.1" ||
    api147.fields.targetName !== "SPEEDYBEEF405V4" ||
    api147.fields.scopeMismatchField !== undefined ||
    api147.openCount !== 2 || api147.closeCount !== 2
  ) throw new Error(`PRODUCTION_API_147_READONLY_PROOF_FAILED:${JSON.stringify(api147)}`);
'''
    replace_once("web/tests/webapp-readonly-fc-browser-smoke.mjs", old_api147, new_api147)

print("M3 API 1.47 read-only state-machine integration patch applied")
