# PR #18 to current-main integration plan

## Frozen comparison

This is analysis only. No merge, rebase, reset, or branch retargeting was performed.

| Item | Exact value |
| --- | --- |
| Current accepted `main` | `4d7cf1ca9fa882b40e7151b96ce6c2dd8806b42b` |
| Current `main` tree | `9829b47c9a6431f4756102df872c1af45b5e4c7d` |
| PR #18 head | `3555a293d7b0ac785bdd040c2e401b7d24f64fcc` |
| PR #18 tree | `01c00d587fcd67725809aceade1ef8aac700c8a8` |
| Merge base | `8ef20be74a34912de53030d28d29b5e4108ddd08` |
| GitHub comparison | diverged; PR #18 side ahead 14, behind 8 |
| PR #18 current state | Draft, open, unmerged, GitHub reports not mergeable |
| Diagnostic child base | exact PR #18 head above |

The eight `main`-side commits are the merged GitHub-native delivery infrastructure from PR #19
and the focused service-worker control correction from PR #20. PR #21 remains a separate Draft on
current `main` and changes only the delivery document, selected-actions policy, and delivery
workflow contract test.

## Overlap inventory

PR #18 and merged PR #19 both change these 12 paths:

- `.github/workflows/ci.yml`
- `scripts/check_webserial_boundary.py`
- `scripts/tests/test_check_webserial_boundary.py`
- `web/public/sw.js`
- `web/src/transport/webserial-readonly-host.mjs`
- `web/tests/authority-boundary.test.mjs`
- `web/tests/browser/webserial-readonly-smoke.mjs`
- `web/tests/build-contract.test.mjs`
- `web/tests/indexeddb-storage-boundary.test.mjs`
- `web/tests/pwa-contract.test.mjs`
- `web/tests/webserial-readonly-browser-smoke.mjs`
- `web/vite.config.ts`

Merged PR #20 additionally changes `web/tests/pwa-contract.test.mjs`, already in that overlap, plus
`web/src/pwa-register.ts` and `web/tests/production-delivery-browser-smoke.mjs`, which PR #18 did
not change.

PR #21 has no path overlap with PR #18 or the diagnostic child. It may be reviewed/merged first or
last, but its exact-main CI and selected-actions administrative follow-up must remain independent
from product integration.

The diagnostic child deliberately changes several overlapping PR #18 files further: the Rust
bridge and generated assets, Web Serial host, connection facade, `App`, styles, Web Serial policy,
and browser/static tests. It must not be retargeted directly to `main` before PR #18 integration.

## Required resolution behavior

The merged result must preserve both sides rather than choosing one whole file:

### Delivery and base paths

- Keep current `main`'s normalized `ADE_WEB_BASE_PATH`, `import.meta.env.BASE_URL`, virtual
  `virtual:ade-web-readonly-serial-wasm` module, repository-scoped worker URL/scope, build SHA,
  versioned caches, update handoff, and root plus `/autonomous-drone-expert/` Chrome gates.
- Replace PR #18's root-absolute `/wasm/` and `/sw.js` assumptions at integration time. The
  connection facade's WASM initialization URL also needs the normalized build base; resolving only
  the host import is insufficient.
- Keep the current main public-artifact gate, no-source-map rule, escaped-request detection, and
  `scripts/prepare_web_wasm.py` staging contract. Extend its allowlists for the accepted PR #18
  generated serial assets rather than bypassing the gate.

### Read-only Rust/WASM product

- Keep PR #18's `ReadonlyIdentification`, typed failure stage/reason, exact four-read bridge,
  checked-in asset provenance, product browser connection facade, and `hardwareObserved=false`.
- Keep the diagnostic child's 32-entry Rust event queue, 200-entry page ring, fixed origins,
  privacy gates, and `takeTraceEvent()` API if the owner accepts that child.
- Regenerate the serial WASM once from the final resolved Rust source with Rust 1.85.0 and the
  isolated `wasm-bindgen-cli-support 0.2.127` tool. Update provenance to the resulting source and
  outputs. Do not hand-merge generated JavaScript or WASM.

### CI and policy

- In `.github/workflows/ci.yml`, retain all current delivery/base-path/update tests and add the PR
  #18 serial WASM build, trusted regeneration comparison, low-level Chrome A–J gate, production
  FC connection gate, and updated policy suite. All eight canonical job names must remain.
- Merge the Web Serial gate semantically: current-main virtual-module/base-path requirements plus
  PR #18 authority locks plus diagnostic fixed-vocabulary/no-sink checks. Recompute byte locks only
  after final source review.
- Keep package and Cargo dependency closures unchanged. Neither branch requires a new dependency.

### Service worker

- Current `main`'s versioned and base-aware `web/public/sw.js` is the behavioral base. Do not
  restore PR #18's old root-only cache list.
- Generated WASM remains a mutable runtime asset under the current worker's base-aware
  `wasm/` rule. Trace event data is page RAM and must never be posted to or stored by the worker.

## Safe execution order for owner approval

1. Re-verify `main`, PR #18, PR #21, and the diagnostic child exact heads/trees and Draft state.
2. Review PR #21 independently. If accepted, merge normally, wait for all eight exact-main jobs,
   and refreeze the new main SHA/tree before product integration.
3. Create an integration worktree from `feat/m2-webapp-readonly-fc-connect`. Make a normal merge
   commit from the then-current `main`; do not rebase or force-push shared history.
4. Resolve the 12 overlapping paths using the behavioral rules above. Do not copy an entire side
   over the other.
5. Build/regenerate the serial WASM through the trusted path, update provenance, and run focused
   authority, privacy, base-path, PWA transition, and Chrome tests at both `/` and the repository
   scope.
6. Run canonical CI once. Keep PR #18 Draft for owner review of the merge commit and exact tree.
7. Merge the updated PR #18 branch into `feat/m2-diagnostic-trace-panel` with a normal merge commit.
   Resolve by retaining the updated base-path/PWA integration plus the child trace model/tests.
   Do not rebase or rewrite the published child.
8. Regenerate again only if that second resolution changes Rust or ABI output. Run the child
   focused suite and canonical CI, then keep the stacked PR Draft.
9. Only after both Drafts are reviewed should the owner decide the final merge order. Delivery
   dispatch and physical USB retest are later, separate approvals.

## Stop conditions

Stop and request owner direction if any expected head moved, dependency locks change, the four
command set changes, a base-path fix requires weakening artifact privacy, generated Linux output
does not reproduce byte-for-byte, or an integration resolution widens serial/write authority.
