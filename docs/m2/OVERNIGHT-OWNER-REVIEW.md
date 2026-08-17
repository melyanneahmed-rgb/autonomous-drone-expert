# Overnight owner review dossier

## Executive outcome

The overnight package remains review-only. Nothing was merged, rebased, force-pushed, marked
Ready, dispatched to delivery, or applied to `main`. PR #17, PR #18, and PR #21 remain Draft,
open, and unmerged. A new stacked Draft is prepared from the exact PR #18 head for the temporary,
privacy-bounded diagnostic trace.

No physical FC operation was performed. The owner's earlier observation remains the full hardware
evidence boundary:

- `PHYSICAL_FC_TEST_ATTEMPTED=YES`
- `READONLY_IDENTITY_COMPLETION=NO`
- `HARDWARE_SUPPORT_VALIDATED=NO`

## Frozen repository state

| Item | Exact state |
| --- | --- |
| Official repository | `melyanneahmed-rgb/autonomous-drone-expert` |
| Accepted `main` | `4d7cf1ca9fa882b40e7151b96ce6c2dd8806b42b` |
| Accepted `main` tree | `9829b47c9a6431f4756102df872c1af45b5e4c7d` |
| Exact-main canonical CI | Run #99 / ID `31972348467`; all eight jobs passed |
| PR #17 | Draft/open/unmerged; `716bc2f7fe39a77e44cda43ba6978ebd0d0ec1e0` |
| PR #18 | Draft/open/unmerged; `3555a293d7b0ac785bdd040c2e401b7d24f64fcc` |
| PR #18 tree | `01c00d587fcd67725809aceade1ef8aac700c8a8` |
| PR #21 | Draft/open/unmerged; `3cb6516e0c93248d1f68d7e09a21e951f34c9e58` |
| PR #21 canonical CI | Run #104 / ID `31974704560`; all eight jobs passed |
| Diagnostic base | exact PR #18 head and tree above |
| Diagnostic canonical CI | recorded in the stacked Draft after the exact published head is tested |

## Work A: delivery allowlist hardening

PR #21 preserves immutable full-length SHA pins for all six delivery actions and treats
`actions/checkout@v7.0.1` as the one temporary canonical-CI exception. Its focused delivery
contract tests passed 4/4 and canonical CI Run #104 passed all eight jobs. The PR changes only:

- `docs/m2/GITHUB-NATIVE-DELIVERY.md`
- `policy/github-actions-allowlist.json`
- `web/tests/delivery-workflows.test.mjs`

No repository Actions administration setting was changed and neither delivery workflow was
dispatched. PR #21 remains Draft for owner review.

## Work B: stacked diagnostic Draft

The child branch is based on exact PR #18 head
`3555a293d7b0ac785bdd040c2e401b7d24f64fcc`. It adds a temporary collapsed panel beneath the
existing connection card and no new connection or write authority.

The trace architecture is deliberately narrow:

- Rust remains authoritative for stage, command, direction, frame decision, and parser reason.
- `discover()` remains zero-argument and issues exactly four Rust-owned empty-payload reads.
- Rust holds at most 32 protocol events; the browser holds at most 200 complete fixed-schema
  events and deterministically evicts oldest first.
- Each selection attempt resets the trace. Consumer snapshots and events are frozen.
- Copy emits only a fixed header and validated fixed-token fields. Clear affects RAM only.
- Browser exceptions are reduced to fixed failure classes and fixed structural origins.
- No frame, payload, identity value, browser error message, path, VID/PID, serial number, port
  name, arbitrary string, stack, logger, storage API, analytics API, beacon, or network sink is
  accepted by the trace model.
- `hardwareObserved` remains `false`; no dependency or lockfile changed.

The implementation also fixes a demonstrated fail-closed cleanup edge: if construction of the
genuine WASM discovery fails after port selection, the selected port is cleaned before returning
the fixed `DIRECTIVE_REFUSAL` origin.

The complete event vocabulary is `SELECT_START`, `SELECT_OK`, `SELECT_FAILED`,
`DISCOVERY_START`, `PORT_OPEN_START`, `PORT_OPEN_OK`, `PORT_OPEN_FAILED`, `DIRECTIVE`,
`TX_START`, `TX_OK`, `TX_FAILED`, `RX_CHUNK`, `RX_FAILED`, `FRAME_ACCEPTED`,
`FRAME_REJECTED`, `IDENTITY_STAGE_OK`, `IDENTITY_STAGE_FAILED`, `PORT_CLOSE_START`,
`PORT_CLOSE_OK`, `PORT_CLOSE_FAILED`, `CLEANUP_START`, `CLEANUP_OK`, `CLEANUP_FAILED`,
`UI_BOUNDARY_FAILED`, `FINAL_OK`, and `FINAL_FAILED`.

The complete origin vocabulary is `PORT_SELECTION`, `DISCOVERY`, `PORT_OPEN`,
`WRITER_ACQUISITION`, `READER_ACQUISITION`, `SERIAL_WRITE`, `SERIAL_READ`,
`SERIAL_TIMEOUT`, `MSP_FRAME`, `IDENTITY_STAGE`, `DIRECTIVE_REFUSAL`, `PORT_CLOSE`,
`READER_CANCEL`, `READER_RELEASE`, `WRITER_RELEASE`, `CLEANUP`, `UI_BOUNDARY`, and
`FINAL_RESULT`.

## Physical test impact

During a separately approved future USB-only attempt, normal connection behavior is unchanged.
After the attempt, the owner may expand `تشخيص الاتصال — مؤقت` beneath the connection card. The
newest fixed-token event is shown first. A host exception may safely appear as, for example,
`failureClass=Unknown origin=PORT_OPEN`; a protocol failure may show
`stage=BOARD_INFO failureReason=TrailingPayload`. Neither representation includes the response,
port, device, exception text, or identity values. `نسخ التشخيص الآمن` copies the same sanitized
tokens and `مسح التشخيص` clears only current-page RAM.

## Work C: Web Serial safety review

The deep review is recorded in `docs/m2/WEB-SERIAL-SAFETY-REVIEW.md`. It traced authority from
React through the connection facade, host, generated WASM, Rust discovery, identity state machine,
MSP accumulator, and cleanup.

The review found no critical Web Serial safety blocker after the child fixes. JavaScript cannot
choose a command, provide a payload, forge an approval, substitute an arbitrary discovery, or
parse MSP. Rust rejects wrong command, direction, reply class, checksum, structure, stage, and
scope. Every terminal browser and cleanup boundary now has a fixed safe origin. Failed attempts do
not preserve reusable reader, writer, port, discovery, or trace authority.

## Work D: integration analysis only

No integration was performed. `docs/m2/PR18-MAIN-INTEGRATION-PLAN.md` freezes the divergence and
resolution rules.

PR #18 and current `main` have merge base
`8ef20be74a34912de53030d28d29b5e4108ddd08`; the PR side is ahead 14 and behind 8. GitHub reports
PR #18 not mergeable. Twelve paths overlap merged PR #19, with one of those also changed by PR
#20. Current `main`'s base-aware/versioned PWA and delivery behavior must be preserved while PR
#18's Rust/WASM read-only connection is integrated semantically. PR #18's root-absolute `/wasm/`
and `/sw.js` assumptions must not survive a repository-scoped Pages integration.

The recommended owner-approved path is a normal merge of the then-current `main` into PR #18,
semantic conflict resolution and WASM regeneration, canonical CI, followed by a normal merge of
the updated PR #18 into the diagnostic child. Shared history must not be rebased or force-pushed.

## Work E: ranked repository audit findings

### Critical

None found.

### High

| Files/evidence | Exact behavior and impact | Overnight disposition / next step |
| --- | --- | --- |
| The 12 paths enumerated in `docs/m2/PR18-MAIN-INTEGRATION-PLAN.md`; GitHub comparison of `main` and PR #18 | PR #18 is ahead 14, behind 8, and not mergeable. Blind conflict resolution could regress accepted PWA/base-path behavior or discard Rust read-only authority. | Not fixed because integration was explicitly analysis-only. Follow the documented normal-merge and semantic-resolution plan. |
| `.github/workflows/web-preview.yml`, `.github/workflows/android-apk.yml`, repository selected-actions administration, PR #21 | Delivery startup was rejected before a job ran because the administration allowlist did not admit the immutable action SHAs. The product was not exercised. | PR #21's policy and tests were hardened and CI is green; administration remains unchanged. Owner must accept PR #21 before a separately approved settings change and delivery dispatch. |

### Medium

| Files/evidence | Exact behavior and impact | Overnight disposition / next step |
| --- | --- | --- |
| `web/src/transport/webserial-readonly-host.mjs`, `web/src/connection/readonly-fc-connection.mjs`, `web/public/sw.js` on PR #18/child | Root-absolute `/wasm/` and `/sw.js` assumptions do not map to repository-scoped Pages. A naïve merge could make the accepted hosted app fail to load serial WASM or register the correct worker. | Deliberately not patched on the divergent child. Preserve current `main`'s normalized base and virtual module during integration, then regenerate/test. |
| `web/src/storage/indexeddb-journal-store.ts`, `web/tests/journal-storage-contract.mjs`, `web/src/storage/wasm-journal-host.mjs` | Some non-serial storage failures remain context-free `Unknown`, which weakens troubleshooting but does not expose data or widen authority. | Deferred outside the diagnostic/Web Serial scope. Open a focused typed storage-origin change later. |

### Low

| Files/evidence | Exact behavior and impact | Overnight disposition / next step |
| --- | --- | --- |
| `web/public/wasm/ade_web_readonly_serial_wasm_bridge.js` generated glue | Upstream initialization/MIME fallback paths use `console.warn`. They receive no serial/device/frame data, but create a superficial logging-policy exception. | Accepted as derived generator behavior, outside the recorder. Review/suppress through the generator in a future focused change; never hand-edit the asset. |
| `web/tests/browser/webserial-readonly-smoke.mjs` and product browser runners | Test failures may print bounded Chrome stderr and allowlisted served-route context. This can aid CI diagnosis but is not product telemetry. | Test-only and no physical device is used. Keep fake inputs synthetic and reassess before any physical-browser log capture. |

### Informational

| Files/evidence | Exact behavior and impact | Overnight disposition / next step |
| --- | --- | --- |
| Rust/Web test fixtures found by repository search | Raw MSP bytes exist in synthetic tests only; no product raw-frame/payload logger was found. | No fix required. Keep policy tests forbidding product sinks. |
| `android/app/src/main/AndroidManifest.xml` and Android policy gates | Android declares Internet only; policy forbids `UsbManager`, native WebView serial bridging, MSP authority, and `WriteApproval`. No native USB path was found. | No fix required. Native Android USB remains an explicitly excluded future milestone. |
| `web/vite.config.ts`, public-artifact policy/tests on current `main` | Production builds have no source maps and current `main` rejects private/unexpected public artifacts. | Preserve these gates during PR #18 integration and extend allowlists only for accepted generated assets. |

## Validation evidence

Local validation for the diagnostic child passed:

- `cargo fmt --all --check`
- focused Rust bridge clippy with `-D warnings`
- Rust bridge tests: 12/12
- workspace check, including Rust 1.85.0 MSRV
- workspace tests: 202/202
- generated serial WASM provenance and byte-for-byte output comparison
- Web Serial authority and privacy gate
- Python policy suite: 98 passed; one environment-only symlink capability skip
- TypeScript check
- direct Vite production build
- Web source tests outside the Windows child-process wrapper: 40 passed
- real Chrome Web Serial adversarial groups A–J
- real Chrome production connection scenarios: five passed

The local Node 24 Windows environment cannot spawn the package-manager `.cmd` from the
`build-contract` wrapper with `shell:false`; the direct equivalent Vite production build passed.
Canonical Linux CI is therefore required for the complete exact-head judgment.

The production browser evidence string remains:

`SOFTWARE_EXERCISED;REAL_CHROME_EXERCISED;PHYSICAL_FC_NOT_TESTED;HARDWARE_OBSERVED=NO`

Generated asset evidence before canonical Linux execution:

- local raw WASM input SHA-256:
  `dd07c8c3184c0dcfea81493e5af07df031738bc2811680ffe942eab46ea33a3e`
- generated JavaScript SHA-256:
  `c383a32030cb6e361bed425be8a7ee1b1872fc8d270b9fdb90b15ab1d6d59f75`
- generated WASM SHA-256:
  `b0d5c7ac3ff543a0036549717ccb9aa56d7671193d3d40b91553189484e6625d`
- generated JavaScript size: 23,057 bytes
- generated WASM size: 62,173 bytes
- `web/package-lock.json` SHA-256:
  `c3015e9454da094d307975921b8aa2c195a15b9dffe0498a9c758b57d922c05d`

## Owner review sequence

1. Review PR #21 independently. Do not change delivery administration or dispatch a workflow
   until that PR is accepted under a separate owner decision.
2. Review the diagnostic stacked Draft for vocabulary, privacy boundary, UI placement, and test
   evidence. Keep it stacked on PR #18.
3. Review the Web Serial safety report and the ranked findings above.
4. Approve or reject the PR #18/main integration plan. No integration has been pre-authorized by
   this package.
5. If integration is approved later, require the exact-head/tree and all eight canonical CI jobs
   before any Ready or merge decision.
6. A new physical USB-only attempt remains a separate explicit owner action after the software
   lineage is accepted. This package does not request or perform it.

The required decisions are therefore short and explicit:

- `DECISION 1 — Accept or reject revised PR #21.`
- `DECISION 2 — Accept or reject the Diagnostic Trace architecture.`
- `DECISION 3 — Approve or reject integration of latest main into PR #18.`
- `DECISION 4 — After accepted integration, approve or reject one physical USB-only retest.`
