#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def replace_once(path: str, old: str, new: str) -> None:
    file = ROOT / path
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one match in {path}, found {count}: {old[:80]!r}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")


# 1. Execution crate gains the already-reviewed read-profile selector/decoder as a first-party dep.
replace_once(
    "crates/execution/Cargo.toml",
    'ade-facts = { path = "../facts" }\n',
    'ade-facts = { path = "../facts" }\nade-readonly-profile = { path = "../readonly-profile" }\n',
)

# 2. Rust-owned identification state machine selects reviewed read profiles instead of using the
#    narrower M1 write-scope gate as a read-continuation gate.
replace_once(
    "crates/execution/src/lib.rs",
    'use ade_facts::{ApiScopeStatus, DeviceIdentity, check_m1_api_scope};\n',
    'use ade_facts::DeviceIdentity;\n',
)
replace_once(
    "crates/execution/src/lib.rs",
    'use ade_protocol_msp::{\n    ApiVersion, BeeperConfigSnapshot, CommandId, Correlator, Direction, FcVariant, FcVersion,\n    Frame, MspError, ReplyClass, SetBeeperConfig, decode_frame, encode_frame,\n};\n',
    'use ade_protocol_msp::{\n    ApiVersion, BeeperConfigSnapshot, CommandId, Correlator, Direction, FcVariant, FcVersion,\n    Frame, MspError, ReplyClass, SetBeeperConfig, decode_frame, encode_frame,\n};\nuse ade_readonly_profile::{\n    ReadonlyApiProfileStatus, ReadonlyFcVersion, ReadonlyProfileCandidate,\n    ReadonlyProfileDecodeError, accept_variant, classify_readonly_api_profile, decode_fc_version,\n};\n',
)
replace_once(
    "crates/execution/src/lib.rs",
    '    /// A response arrived without an identification request in flight.\n    NoIdentificationRequestPending,\n    /// The identification sequence has reached a terminal supported or unsupported result.\n',
    '    /// A response arrived without an identification request in flight.\n    NoIdentificationRequestPending,\n    /// An internal typed identification fact required by the current read profile is absent.\n    IdentificationStateMissing {\n        /// Stable internal field label; never a device value.\n        field: &\'static str,\n    },\n    /// The read-profile variant gate changed unexpectedly between stages.\n    ReadProfileVariantMismatch,\n    /// The identification sequence has reached a terminal supported or unsupported result.\n',
)
replace_once(
    "crates/execution/src/lib.rs",
    '    api: Option<ApiVersion>,\n    variant: Option<FcVariant>,\n    version: Option<FcVersion>,\n',
    '    api: Option<ApiVersion>,\n    profile: Option<ReadonlyProfileCandidate>,\n    variant: Option<FcVariant>,\n    version: Option<FcVersion>,\n',
)
replace_once(
    "crates/execution/src/lib.rs",
    '            api: None,\n            variant: None,\n            version: None,\n',
    '            api: None,\n            profile: None,\n            variant: None,\n            version: None,\n',
)
replace_once(
    "crates/execution/src/lib.rs",
    '''            IdentificationStage::ApiVersion => {\n                let api = ApiVersion::from_reply(frame)?;\n                match check_m1_api_scope(&api) {\n                    ApiScopeStatus::InScope => {\n                        self.api = Some(api);\n                        self.stage = IdentificationStage::FcVariant;\n                        Ok(IdentificationProgress::Pending)\n                    }\n                    ApiScopeStatus::Mismatch { field } => {\n                        self.stage = IdentificationStage::Complete;\n                        Ok(IdentificationProgress::ApiScopeMismatch { api, field })\n                    }\n                }\n            }\n            IdentificationStage::FcVariant => {\n                self.variant = Some(FcVariant::from_reply(frame)?);\n                self.stage = IdentificationStage::FcVersion;\n                Ok(IdentificationProgress::Pending)\n            }\n            IdentificationStage::FcVersion => {\n                self.version = Some(FcVersion::from_reply(frame)?);\n                self.stage = IdentificationStage::BoardInfo;\n                Ok(IdentificationProgress::Pending)\n            }\n''',
    '''            IdentificationStage::ApiVersion => {\n                let api = ApiVersion::from_reply(frame)?;\n                match classify_readonly_api_profile(\n                    api.protocol_version,\n                    api.api_major,\n                    api.api_minor,\n                ) {\n                    ReadonlyApiProfileStatus::Candidate(profile) => {\n                        self.api = Some(api);\n                        self.profile = Some(profile);\n                        self.stage = IdentificationStage::FcVariant;\n                        Ok(IdentificationProgress::Pending)\n                    }\n                    ReadonlyApiProfileStatus::UnsupportedProtocol => {\n                        self.stage = IdentificationStage::Complete;\n                        Ok(IdentificationProgress::ApiScopeMismatch {\n                            api,\n                            field: "protocol_version",\n                        })\n                    }\n                    ReadonlyApiProfileStatus::UnsupportedApi => {\n                        self.stage = IdentificationStage::Complete;\n                        Ok(IdentificationProgress::ApiScopeMismatch {\n                            api,\n                            field: "msp_api_version",\n                        })\n                    }\n                }\n            }\n            IdentificationStage::FcVariant => {\n                let variant = FcVariant::from_reply(frame)?;\n                let profile = self.profile.ok_or(ExecError::IdentificationStateMissing {\n                    field: "read_profile",\n                })?;\n                if !accept_variant(profile, variant.identifier) {\n                    let api = self.api.take().ok_or(ExecError::IdentificationStateMissing {\n                        field: "api",\n                    })?;\n                    self.stage = IdentificationStage::Complete;\n                    return Ok(IdentificationProgress::ApiScopeMismatch {\n                        api,\n                        field: "fc_variant",\n                    });\n                }\n                self.variant = Some(variant);\n                self.stage = IdentificationStage::FcVersion;\n                Ok(IdentificationProgress::Pending)\n            }\n            IdentificationStage::FcVersion => {\n                let profile = self.profile.ok_or(ExecError::IdentificationStateMissing {\n                    field: "read_profile",\n                })?;\n                let variant = self\n                    .variant\n                    .as_ref()\n                    .ok_or(ExecError::IdentificationStateMissing { field: "fc_variant" })?;\n                let decoded = decode_fc_version(profile, variant.identifier, frame).map_err(\n                    |error| match error {\n                        ReadonlyProfileDecodeError::VariantMismatch => {\n                            ExecError::ReadProfileVariantMismatch\n                        }\n                        ReadonlyProfileDecodeError::Protocol(error) => ExecError::Payload(error),\n                    },\n                )?;\n                self.version = Some(match decoded {\n                    ReadonlyFcVersion::Legacy(version) => version,\n                    ReadonlyFcVersion::CalendarExtended {\n                        calendar_version, ..\n                    } => FcVersion {\n                        major: calendar_version[0],\n                        minor: calendar_version[1],\n                        patch: calendar_version[2],\n                    },\n                });\n                self.stage = IdentificationStage::BoardInfo;\n                Ok(IdentificationProgress::Pending)\n            }\n''',
)
replace_once(
    "crates/execution/src/lib.rs",
    '    fn unsupported_api_versions_end_the_sequence_after_the_first_empty_read() {\n        for (payload, field) in [\n            ([0, 1, 45], "msp_api_version"),\n            ([0, 1, 47], "msp_api_version"),\n            ([0, 2, 46], "msp_api_version"),\n            ([1, 1, 46], "protocol_version"),\n        ] {\n',
    '    fn unreviewed_api_versions_end_the_sequence_after_the_first_empty_read() {\n        for (payload, field) in [\n            ([0, 1, 45], "msp_api_version"),\n            ([0, 1, 48], "msp_api_version"),\n            ([0, 2, 46], "msp_api_version"),\n            ([1, 1, 46], "protocol_version"),\n        ] {\n',
)
replace_once(
    "crates/execution/src/lib.rs",
    '''    #[test]\n    fn incremental_identification_has_an_exhaustive_context_allowlist() {\n''',
    '''    #[test]\n    fn reviewed_api_147_betaflight_completes_the_four_read_identity_without_write_scope() {\n        fn board_payload() -> Vec<u8> {\n            let mut payload = Vec::new();\n            payload.extend_from_slice(b"F405");\n            payload.extend_from_slice(&0u16.to_le_bytes());\n            payload.extend_from_slice(&[0, 0]);\n            for value in ["SPEEDYBEEF405V4", "SpeedyBee F405 V4", "SPB"] {\n                payload.push(u8::try_from(value.len()).unwrap());\n                payload.extend_from_slice(value.as_bytes());\n            }\n            payload.extend_from_slice(&[0; ade_protocol_msp::SIGNATURE_LENGTH]);\n            payload.extend_from_slice(&[0, 0]);\n            payload.extend_from_slice(&0u16.to_le_bytes());\n            payload.extend_from_slice(&0u32.to_le_bytes());\n            payload.extend_from_slice(&[0, 0]);\n            payload\n        }\n\n        let mut identification = ReadonlyIdentification::new(SessionState::Identifying).unwrap();\n        let replies = [\n            (CommandId::ApiVersion, vec![0, 1, 47]),\n            (CommandId::FcVariant, b"BTFL".to_vec()),\n            (\n                CommandId::FcVersion,\n                [vec![25, 12, 1, 9], b"2025.12.1".to_vec()].concat(),\n            ),\n            (CommandId::BoardInfo, board_payload()),\n        ];\n        let mut commands = Vec::new();\n        let mut completed = None;\n        for (command, payload) in replies {\n            let request = identification.next_request().unwrap();\n            commands.push(request.command());\n            assert_eq!(request.command(), command);\n            let reply = decode_frame(\n                &encode_frame(Direction::Reply, command, &payload).unwrap(),\n            )\n            .unwrap();\n            match identification.accept_response(&reply).unwrap() {\n                IdentificationProgress::Pending => {}\n                IdentificationProgress::Complete(identity) => completed = Some(identity),\n                IdentificationProgress::ApiScopeMismatch { .. } => {\n                    panic!("reviewed API 1.47 BTFL read profile must not stop early")\n                }\n            }\n        }\n\n        assert_eq!(\n            commands,\n            [\n                CommandId::ApiVersion,\n                CommandId::FcVariant,\n                CommandId::FcVersion,\n                CommandId::BoardInfo,\n            ]\n        );\n        let identity = completed.expect("API 1.47 identity must complete");\n        assert_eq!(identity.api.api_minor, 47);\n        assert_eq!(&identity.variant.identifier, b"BTFL");\n        assert_eq!(\n            (identity.version.major, identity.version.minor, identity.version.patch),\n            (25, 12, 1),\n        );\n        assert_eq!(identity.target_name, "SPEEDYBEEF405V4");\n        assert_eq!(\n            ade_facts::check_m1_api_scope(&identity.api),\n            ade_facts::ApiScopeStatus::Mismatch {\n                field: "msp_api_version"\n            },\n        );\n        assert!(identification.is_complete());\n    }\n\n    #[test]\n    fn reviewed_api_147_rejects_non_betaflight_variant_before_version_read() {\n        let mut identification = ReadonlyIdentification::new(SessionState::Identifying).unwrap();\n        let api_request = identification.next_request().unwrap();\n        assert_eq!(api_request.command(), CommandId::ApiVersion);\n        let api_reply = decode_frame(\n            &encode_frame(Direction::Reply, CommandId::ApiVersion, &[0, 1, 47]).unwrap(),\n        )\n        .unwrap();\n        assert_eq!(\n            identification.accept_response(&api_reply).unwrap(),\n            IdentificationProgress::Pending,\n        );\n\n        let variant_request = identification.next_request().unwrap();\n        assert_eq!(variant_request.command(), CommandId::FcVariant);\n        let variant_reply = decode_frame(\n            &encode_frame(Direction::Reply, CommandId::FcVariant, b"INAV").unwrap(),\n        )\n        .unwrap();\n        assert_eq!(\n            identification.accept_response(&variant_reply).unwrap(),\n            IdentificationProgress::ApiScopeMismatch {\n                api: ApiVersion {\n                    protocol_version: 0,\n                    api_major: 1,\n                    api_minor: 47,\n                },\n                field: "fc_variant",\n            },\n        );\n        assert!(identification.is_complete());\n        assert_eq!(\n            identification.next_request().unwrap_err(),\n            ExecError::IdentificationAlreadyComplete,\n        );\n    }\n\n    #[test]\n    fn incremental_identification_has_an_exhaustive_context_allowlist() {\n''',
)

# 3. WebAssembly facade tests: 1.47 is no longer an early API stop; it completes read-only identity
#    and then remains outside the still-exact M1 write/product scope.
replace_once(
    "crates/web-readonly-serial-wasm-bridge/src/lib.rs",
    '            ([0, 1, 45], "1.45", "msp_api_version"),\n            ([0, 1, 47], "1.47", "msp_api_version"),\n            ([0, 2, 46], "2.46", "msp_api_version"),\n',
    '            ([0, 1, 45], "1.45", "msp_api_version"),\n            ([0, 1, 48], "1.48", "msp_api_version"),\n            ([0, 2, 46], "2.46", "msp_api_version"),\n',
)
replace_once(
    "crates/web-readonly-serial-wasm-bridge/src/lib.rs",
    '''    #[test]\n    fn refused_identification_contexts_cannot_create_a_web_serial_directive() {\n''',
    '''    #[test]\n    fn reviewed_api_147_betaflight_runs_all_four_reads_and_returns_bounded_identity() {\n        let mut bridge = WasmReadonlySerialDiscovery::create().unwrap();\n        let open = bridge.begin_open().unwrap();\n        let api = bridge.accept_open_ok(&open.request_id).unwrap();\n        let variant = feed_reply(\n            &mut bridge,\n            &api,\n            Direction::Reply,\n            CommandId::ApiVersion,\n            &[0, 1, 47],\n        );\n        let version = feed_reply(\n            &mut bridge,\n            &variant,\n            Direction::Reply,\n            CommandId::FcVariant,\n            b"BTFL",\n        );\n        let mut version_payload = vec![25, 12, 1, 9];\n        version_payload.extend_from_slice(b"2025.12.1");\n        let board = feed_reply(\n            &mut bridge,\n            &version,\n            Direction::Reply,\n            CommandId::FcVersion,\n            &version_payload,\n        );\n        let close = feed_reply(\n            &mut bridge,\n            &board,\n            Direction::Reply,\n            CommandId::BoardInfo,\n            &valid_board_payload(),\n        );\n        assert_eq!(close.kind, "close");\n        assert!(bridge.identity().is_some());\n        assert_eq!(bridge.api_version().as_deref(), Some("1.47"));\n        assert_eq!(bridge.fc_variant().as_deref(), Some("BTFL"));\n        assert_eq!(bridge.fc_version().as_deref(), Some("25.12.1"));\n        assert_eq!(bridge.target_name().as_deref(), Some("SPEEDYBEEF405V4"));\n        assert_eq!(\n            bridge.scope_mismatch_field().as_deref(),\n            Some("msp_api_version"),\n        );\n        bridge.accept_close(&close.request_id, None).unwrap();\n        assert_eq!(bridge.outcome_kind(), "scope-mismatch");\n        assert!(!bridge.hardware_observed());\n        assert_eq!(\n            bridge\n                .trace_events\n                .iter()\n                .filter(|event| event.event == "DIRECTIVE")\n                .count(),\n            4,\n        );\n    }\n\n    #[test]\n    fn refused_identification_contexts_cannot_create_a_web_serial_directive() {\n''',
)

# 4. Low-level real-Chrome host test now treats API 1.47 as a reviewed four-read profile.
replace_once(
    "web/tests/browser/webserial-readonly-smoke.mjs",
    '].map((bytes) => Uint8Array.from(bytes));\nconst API_SCOPE_REPLIES = [\n',
    '].map((bytes) => Uint8Array.from(bytes));\nconst API_147_REPLIES = [\n  Uint8Array.from([36, 77, 62, 3, 1, 0, 1, 47, 44]),\n  IN_SCOPE_REPLIES[1],\n  Uint8Array.from([36, 77, 62, 13, 3, 25, 12, 1, 9, 50, 48, 50, 53, 46, 49, 50, 46, 49, 36]),\n  IN_SCOPE_REPLIES[3],\n];\nconst API_SCOPE_REPLIES = [\n',
)
replace_once(
    "web/tests/browser/webserial-readonly-smoke.mjs",
    '  ["lower-minor", [36, 77, 62, 3, 1, 0, 1, 45, 46], "1.45", "normal"],\n  ["higher-minor", [36, 77, 62, 3, 1, 0, 1, 47, 44], "1.47", "split-frame"],\n  ["incompatible-major", [36, 77, 62, 3, 1, 0, 2, 46, 46], "2.46", "whole-frame"],\n',
    '  ["lower-minor", [36, 77, 62, 3, 1, 0, 1, 45, 46], "1.45", "normal"],\n  ["higher-unreviewed-minor", [36, 77, 62, 3, 1, 0, 1, 48, 43], "1.48", "split-frame"],\n  ["incompatible-major", [36, 77, 62, 3, 1, 0, 2, 46, 46], "2.46", "whole-frame"],\n',
)
replace_once(
    "web/tests/browser/webserial-readonly-smoke.mjs",
    '''  const full = await runDiscovery(FULL_SCOPE_MISMATCH_REPLIES);\n''',
    '''  const api147 = await runDiscovery(API_147_REPLIES, "split-frame");\n  assert(api147.result.outcome === "scope-mismatch", "API 1.47 completes read-only identity");\n  assert(api147.result.apiVersion === "1.47", "API 1.47 fact retained");\n  assert(api147.result.fcVariant === "BTFL", "API 1.47 variant retained");\n  assert(api147.result.fcVersion === "25.12.1", "API 1.47 calendar triplet retained");\n  assert(api147.result.targetName === "SPEEDYBEEF405V4", "API 1.47 target retained");\n  assert(\n    api147.result.scopeMismatchField === "msp_api_version",\n    "API 1.47 remains outside M1 write/product scope",\n  );\n  assert(api147.port.writes.length === 4, "API 1.47 sends exactly the four read-only requests");\n  EXPECTED_REQUESTS.forEach((expected, index) =>\n    assert(equalBytes(api147.port.writes[index], expected), `API 1.47 request order ${index}`),\n  );\n  assert(api147.port.closeCount === 1, "API 1.47 closes exactly once");\n\n  const full = await runDiscovery(FULL_SCOPE_MISMATCH_REPLIES);\n''',
)
replace_once(
    "web/tests/browser/webserial-readonly-smoke.mjs",
    '  const port = new TestPort([Uint8Array.from(API_SCOPE_REPLIES[1][1])], "split-frame");\n',
    '  const port = new TestPort([Uint8Array.from(API_SCOPE_REPLIES[0][1])], "split-frame");\n',
)

# 5. Production UI real-browser test exercises the new API 1.47 four-read path twice.
replace_once(
    "web/tests/webapp-readonly-fc-browser-smoke.mjs",
    '  ].map((bytes) => Uint8Array.from(bytes));\n  const boardPayloadWithTrailingByte = [...inScopeReplies[3].slice(5, -1), 0];\n',
    '  ].map((bytes) => Uint8Array.from(bytes));\n  const api147Replies = [\n    Uint8Array.from([36, 77, 62, 3, 1, 0, 1, 47, 44]),\n    inScopeReplies[1],\n    Uint8Array.from([36, 77, 62, 13, 3, 25, 12, 1, 9, 50, 48, 50, 53, 46, 49, 50, 46, 49, 36]),\n    inScopeReplies[3],\n  ];\n  const boardPayloadWithTrailingByte = [...inScopeReplies[3].slice(5, -1), 0];\n',
)
replace_once(
    "web/tests/webapp-readonly-fc-browser-smoke.mjs",
    '''  const replies = scenario === "api-unsupported-147" || scenario === "fragmented-api-version"\n    ? [Uint8Array.from([36, 77, 62, 3, 1, 0, 1, 47, 44])]\n    : scenario === "api-unsupported-major"\n''',
    '''  const replies = scenario === "api147-readonly-complete"\n    ? api147Replies\n    : scenario === "fragmented-api-version"\n      ? [Uint8Array.from([36, 77, 62, 3, 1, 0, 1, 45, 46])]\n      : scenario === "api-unsupported-major"\n''',
)
replace_once(
    "web/tests/webapp-readonly-fc-browser-smoke.mjs",
    '''  const api147 = await runScenario(\n    browser,\n    url,\n    "api-unsupported-147",\n    "api-unsupported",\n    2,\n  );\n  if (\n    JSON.stringify(api147.writes) !== JSON.stringify([expectedRequests[0], expectedRequests[0]]) ||\n    Object.keys(api147.fields).length !== 0 ||\n    api147.openCount !== 2 || api147.closeCount !== 2 ||\n    api147.text.includes("1.47") || api147.text.includes("msp_api_version")\n  ) throw new Error(`PRODUCTION_API_147_GATE_FAILED:${JSON.stringify(api147)}`);\n''',
    '''  const api147 = await runScenario(\n    browser,\n    url,\n    "api147-readonly-complete",\n    "scope-mismatch",\n    2,\n  );\n  if (\n    JSON.stringify(api147.writes) !== JSON.stringify([...expectedRequests, ...expectedRequests]) ||\n    api147.fields.apiVersion !== "1.47" || api147.fields.fcVariant !== "BTFL" ||\n    api147.fields.fcVersion !== "25.12.1" ||\n    api147.fields.targetName !== "SPEEDYBEEF405V4" ||\n    api147.fields.scopeMismatchField !== "msp_api_version" ||\n    api147.openCount !== 2 || api147.closeCount !== 2\n  ) throw new Error(`PRODUCTION_API_147_READONLY_PROOF_FAILED:${JSON.stringify(api147)}`);\n''',
)

# 6. Ordinary UI no longer calls a fully-read-but-write-ineligible identity "unsupported".
replace_once(
    "web/src/App.tsx",
    '  "scope-mismatch": "اكتملت القراءة — الهوية خارج النطاق المقترح",\n',
    '  "scope-mismatch": "اكتملت القراءة بأمان — الكتابة غير متاحة لهذه الهوية",\n',
)

# 7. M3 documentation records the production-state-machine integration under review.
replace_once(
    "docs/m3/README.md",
    '**Status:** slices 1–4 merged; strict API 1.47 read-only decoder in review\n',
    '**Status:** slices 1–5 merged; API 1.47 Rust-owned identification integration in review\n',
)
replace_once(
    "docs/m3/README.md",
    '## Safety properties\n',
    '''## Slice 6 — Rust-owned API 1.47 identification integration\n\nThe production `ReadonlyIdentification` state machine now uses the read-profile registry to decide\nwhether a structurally valid API tuple may continue **read-only** identity. Protocol 0 / API 1.47\ntherefore proceeds through the same four fixed empty-payload reads as API 1.46, but only after the\nexact `BTFL` variant gate passes. The profile-specific extended `FC_VERSION` decoder then runs,\n`BOARD_INFO` completes the identity, and the connection closes normally.\n\nThis does not change M1 write eligibility. The completed API 1.47 identity is still classified by\nthe separate product/write-scope check as `msp_api_version` mismatch, so the Web result exposes the\nbounded identity while every hardware write remains unavailable. Unknown API tuples still stop\nafter `MSP_API_VERSION`; a non-`BTFL` variant stops after `MSP_FC_VARIANT`; neither path can reach a\nprofile-specific version decoder.\n\nThe calendar triplet is retained in the existing three-byte identity version fields (`25.12.1` for\nthe pinned 2025.12.1 profile). The strict upstream version string is validated by the profile\ndecoder but is not persisted into the stable reconnect identity in this slice.\n\nNo physical evidence is changed by this software integration.\n\n## Safety properties\n''',
)
replace_once(
    "docs/m3/README.md",
    '''1. Integrate the reviewed API 1.47 read profile into the Rust-owned identification state machine so\n   `MSP_API_VERSION` selects a read candidate, `MSP_FC_VARIANT` gates it, and only then the\n   profile-specific `MSP_FC_VERSION` decoder runs.\n2. Preserve the exact M1 write-scope check independently: API 1.47 must remain write-ineligible even\n   if read-only identity becomes complete.\n3. Add capability/profile selection evidence to the bounded Web diagnostic/result model without\n   exposing firmware-engine details in the ordinary product UI.\n4. Keep all real writes blocked until the later write milestone and a separate owner approval.\n''',
    '''1. Add capability/profile selection evidence to the bounded Web diagnostic/result model without\n   exposing firmware-engine details in the ordinary product UI.\n2. Add a review-only Betaflight 2025.12.1 capability descriptor only after its exact target/version\n   selection policy is separately reviewed; read compatibility must never imply write support.\n3. Keep the exact M1 write-scope check independent: API 1.47 remains write-ineligible.\n4. Keep all real writes blocked until the later write milestone and a separate owner approval.\n''',
)

print("M3 API 1.47 identification source integration staged")
