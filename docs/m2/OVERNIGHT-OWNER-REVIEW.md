# Overnight owner review dossier

## Executive outcome

This dossier now records the correction-only pass after repository state changed. PR #21 was
merged normally into `main`. PR #23 was merged earlier than intended into PR #18 as
`fa92e5aaade68d962c74843f7dbae6325fe3db2c`; that merge is retained and was not reverted. PR #18
remains Draft/open/unmerged. This pass changes only the diagnostic identity, receive-direction
semantics, tests, and current Git evidence on the PR #18 branch. It does not integrate `main`, mark
Ready, dispatch delivery, change repository settings, or perform a physical FC operation.

No physical FC operation was performed. The owner's earlier observation remains the full hardware
evidence boundary:

- `PHYSICAL_FC_TEST_ATTEMPTED=YES`
- `READONLY_IDENTITY_COMPLETION=NO`
- `HARDWARE_SUPPORT_VALIDATED=NO`

## Frozen repository state

| Item | Exact state |
| --- | --- |
| Official repository | `melyanneahmed-rgb/autonomous-drone-expert` |
| Accepted `main` | `cea776b5c00444289eca95a255c0ec79d22eaaeb` |
| Accepted `main` tree | `a89a3cbdf4339960c872b731f7e884b14974cdbe` |
| Exact-main canonical CI | Run #109 / ID `32057428446`; all eight jobs passed |
| PR #17 | Draft/open/unmerged; `716bc2f7fe39a77e44cda43ba6978ebd0d0ec1e0` |
| PR #18 correction-start state | Draft/open/unmerged; `fa92e5aaade68d962c74843f7dbae6325fe3db2c` |
| PR #18 correction-start tree | `8823a42a75a2a5f7b8db8b6c68e8acede51e63c7` |
| PR #18 correction-start CI | Run #110 / ID `32057454110`; all eight jobs passed |
| PR #21 | merged into `main` as `cea776b5c00444289eca95a255c0ec79d22eaaeb` |
| PR #23 | merged into PR #18 as `fa92e5aaade68d962c74843f7dbae6325fe3db2c` |
| Merge base | `8ef20be74a34912de53030d28d29b5e4108ddd08` |
| Divergence | PR #18 ahead 21 / behind 19 at correction start |
| Overlap | 11 paths, enumerated in `PR18-MAIN-INTEGRATION-PLAN.md` |

## Work A: merged delivery allowlist context

Merged PR #21 preserves immutable full-length SHA pins for all six delivery actions and treats
`actions/checkout@v7.0.1` as the one temporary canonical-CI exception. Its focused delivery
contract tests passed 4/4 and canonical CI Run #104 passed all eight jobs. The PR changes only:

- `docs/m2/GITHUB-NATIVE-DELIVERY.md`
- `policy/github-actions-allowlist.json`
- `web/tests/delivery-workflows.test.mjs`

No repository Actions administration setting or delivery workflow is changed by this
correction-only pass.

## Work B: diagnostic merged into PR #18 and corrected in place

PR #23 merged its reviewed head `f908f77ec88befc45a21e6aaa8491c5f7f21bdb3` into the prior PR
#18 head `3555a293d7b0ac785bdd040c2e401b7d24f64fcc`. The resulting merge commit is
`fa92e5aaade68d962c74843f7dbae6325fe3db2c`, and its tree is
`8823a42a75a2a5f7b8db8b6c68e8acede51e63c7`. The temporary collapsed panel is therefore now part
of PR #18. This correction retains that merge and adds no connection or write authority.

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

## Work D: refreshed integration analysis only

No `main` integration was performed. `docs/m2/PR18-MAIN-INTEGRATION-PLAN.md` freezes the refreshed
divergence and resolution rules.

PR #18 and current `main` have merge base
`8ef20be74a34912de53030d28d29b5e4108ddd08`; the PR side is ahead 21 and behind 19. GitHub reports
PR #18 not mergeable. Eleven paths overlap current `main` and PR #18. Current `main`'s
base-aware/versioned PWA and delivery behavior must be preserved while PR
#18's Rust/WASM read-only connection is integrated semantically. PR #18's root-absolute `/wasm/`
and `/sw.js` assumptions must not survive a repository-scoped Pages integration.

The recommended owner-approved path is a normal merge of the then-current `main` into PR #18,
semantic conflict resolution and WASM regeneration only if required, followed by canonical CI.
Shared history must not be rebased or force-pushed. That integration remains a future owner
decision.

## Work E: ranked repository audit findings

### Critical

None found.

### High

| Files/evidence | Exact behavior and impact | Overnight disposition / next step |
| --- | --- | --- |
| The 11 paths enumerated in `docs/m2/PR18-MAIN-INTEGRATION-PLAN.md`; GitHub comparison of `main` and PR #18 | PR #18 is ahead 21, behind 19, and not mergeable. Blind conflict resolution could regress accepted PWA/base-path behavior or discard Rust read-only authority. | Not fixed because integration is explicitly analysis-only. Follow the documented normal-merge and semantic-resolution plan after separate owner approval. |
| `.github/workflows/web-preview.yml`, `.github/workflows/android-apk.yml`, repository selected-actions administration | Delivery administration and dispatch are separate from this correction-only pass. | PR #21 is merged; this correction does not change settings or dispatch delivery. |

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

The correction-only pass was validated again after changing the diagnostic header and browser RX
direction semantics:

- diagnostic recorder/copy tests: 4/4 passed, including the exact
  `ADE_READONLY_DIAGNOSTIC_TRACE_V1` header and forbidden-identifier regression
- TypeScript and the nine focused connection/authority source contracts passed
- Web source tests: 40/41 passed locally; only the documented Windows Node 24 `.cmd` spawn wrapper
  failed, while its direct production build passed
- workspace Rust formatting, clippy with `-D warnings`, and 202 workspace tests passed
- Rust 1.85.0 workspace MSRV check passed; the isolated tool compiled on Rust 1.97.1 because this
  sandbox could not install the absent Rust 1.86.0 toolchain, so canonical Linux CI must prove its
  declared 1.86.0 MSRV
- Web Serial authority, dependency, isolation, unsafe-Rust, provenance, generated-WASM, and secret
  gates passed; the Python policy suite passed 99 tests with one environment-only symlink skip
- real Chrome Rust/WASM Web Serial groups A–J passed, including Rust-owned `REPLY`/`ERROR`,
  malformed/fragmented input, per-stage timeout/disconnect, retry, cleanup, and privacy attacks
- real Chrome production connection scenarios passed with
  `SOFTWARE_EXERCISED;REAL_CHROME_EXERCISED;PHYSICAL_FC_NOT_TESTED;HARDWARE_OBSERVED=NO`

The committed generated JavaScript, canonical Linux WASM, and package lock retained the exact
hashes recorded below. No Rust ABI, generated asset, dependency manifest, or lockfile changed.

Historical local validation for the diagnostic work before its merge into PR #18 passed:

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

Generated asset evidence:

- local raw WASM input SHA-256:
  `dd07c8c3184c0dcfea81493e5af07df031738bc2811680ffe942eab46ea33a3e`
- generated JavaScript SHA-256:
  `c383a32030cb6e361bed425be8a7ee1b1872fc8d270b9fdb90b15ab1d6d59f75`
- rejected local Windows generated WASM SHA-256:
  `b0d5c7ac3ff543a0036549717ccb9aa56d7671193d3d40b91553189484e6625d`
- trusted canonical Linux generated WASM SHA-256:
  `3ddaed07385e83d02b68d5f3269c9c98ce009da12dae8d6415120ac70a2b3d2f`
- generated JavaScript size: 23,057 bytes
- generated WASM size: 62,173 bytes
- `web/package-lock.json` SHA-256:
  `c3015e9454da094d307975921b8aa2c195a15b9dffe0498a9c758b57d922c05d`

Canonical Run #107 exposed the Linux output through a temporary failure-only diagnostic after
Run #106 proved the Windows-generated WASM did not reproduce. The recovered artifact had the
expected WASM v1 header, exact reported SHA-256, and unchanged size. The diagnostic code is absent
from the final head; the final canonical run must reproduce the committed Linux bytes directly.

## Owner review sequence

1. Review the corrected PR #18 identity, RX-direction semantics, privacy boundary, and exact-head
   CI while keeping PR #18 Draft/open/unmerged.
2. Review the Web Serial safety report and refreshed integration findings above.
3. Approve or reject the PR #18/main integration plan separately. No integration has been
   pre-authorized by this package.
4. If integration is approved later, require the exact-head/tree and all eight canonical CI jobs
   before any Ready or merge decision.
5. A new physical USB-only attempt remains a separate explicit owner action after the software
   lineage is accepted. This package does not request or perform it.

The required decisions are therefore short and explicit:

- `DECISION 1 — Accept or reject the corrected Diagnostic Trace on PR #18.`
- `DECISION 2 — Approve or reject integration of latest main into PR #18.`
- `DECISION 3 — After accepted integration, approve or reject one physical USB-only retest.`
