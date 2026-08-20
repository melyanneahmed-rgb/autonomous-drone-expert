use ade_execution::{IdentificationProgress, ReadonlyIdentification};
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
        .accept_response(&reply(
            CommandId::BoardInfo,
            &board_payload("SPEEDYBEEF405V5"),
        ))
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
