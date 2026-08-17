# PR #18 to current-main integration plan

## Correction-start comparison

This is analysis only. No merge, rebase, reset, or branch retargeting was performed.

| Item | Exact value |
| --- | --- |
| Current accepted `main` | `cea776b5c00444289eca95a255c0ec79d22eaaeb` |
| Current `main` tree | `a89a3cbdf4339960c872b731f7e884b14974cdbe` |
| PR #18 correction-start head | `fa92e5aaade68d962c74843f7dbae6325fe3db2c` |
| PR #18 correction-start tree | `8823a42a75a2a5f7b8db8b6c68e8acede51e63c7` |
| PR #23 merge commit into PR #18 | `fa92e5aaade68d962c74843f7dbae6325fe3db2c` |
| Merge base | `8ef20be74a34912de53030d28d29b5e4108ddd08` |
| GitHub comparison | diverged; PR #18 side ahead 21, behind 19 |
| PR #18 current state | Draft, open, unmerged, GitHub reports not mergeable |
| Correction publication | the post-correction head/tree are recorded in PR #18 and exact-head CI because a commit cannot truthfully embed its own SHA |

The 19 `main`-side commits include the merged GitHub-native delivery infrastructure from PR #19,
the focused service-worker control correction from PR #20, and the selected-actions policy from
merged PR #21. PR #23 is no longer a stacked unmerged child: GitHub merged it normally into PR #18
with first parent `3555a293d7b0ac785bdd040c2e401b7d24f64fcc` and second parent
`f908f77ec88befc45a21e6aaa8491c5f7f21bdb3`. That merge is retained.

## Overlap inventory

Current `main` and PR #18 both change these 11 paths from the verified merge base:

- `.github/workflows/ci.yml`
- `scripts/check_webserial_boundary.py`
- `scripts/tests/test_check_webserial_boundary.py`
- `web/public/sw.js`
- `web/src/transport/webserial-readonly-host.mjs`
- `web/tests/authority-boundary.test.mjs`
- `web/tests/browser/webserial-readonly-smoke.mjs`
- `web/tests/build-contract.test.mjs`
- `web/tests/pwa-contract.test.mjs`
- `web/tests/webserial-readonly-browser-smoke.mjs`
- `web/vite.config.ts`

Merged PR #20 additionally changes `web/tests/pwa-contract.test.mjs`, already in that overlap, plus
`web/src/pwa-register.ts` and `web/tests/production-delivery-browser-smoke.mjs`, which PR #18 did
not change.

Merged PR #21 has no path overlap with PR #18. Its selected-actions policy and any delivery
administration remain independent from product integration and are not changed by this correction.

The diagnostic work merged by PR #23 changes the Rust bridge and generated assets, Web Serial host,
connection facade, `App`, styles, Web Serial policy, and browser/static tests on PR #18. PR #18 must
not be merged to `main` before the separately approved integration pass.

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
- Keep the merged diagnostic's 32-entry Rust event queue, 200-entry page ring, fixed origins,
  privacy gates, and `takeTraceEvent()` API.
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

1. Re-verify `main` and corrected PR #18 exact heads/trees and confirm PR #18 remains Draft.
2. Review the correction-only identity, RX-direction, privacy, authority, and exact-head CI
   evidence. Do not merge `main` during this review.
3. Only after a separate owner approval, create an integration worktree from
   `feat/m2-webapp-readonly-fc-connect` and make a normal merge
   commit from the then-current `main`; do not rebase or force-push shared history.
4. Resolve the 11 overlapping paths using the behavioral rules above. Do not copy an entire side
   over the other.
5. Build/regenerate the serial WASM through the trusted path, update provenance, and run focused
   authority, privacy, base-path, PWA transition, and Chrome tests at both `/` and the repository
   scope.
6. Run canonical CI once. Keep PR #18 Draft for owner review of the merge commit and exact tree.
7. Keep the integrated PR #18 Draft for owner review. Delivery dispatch and physical USB retest
   remain later, separate approvals.

## Stop conditions

Stop and request owner direction if any expected head moved, dependency locks change, the four
command set changes, a base-path fix requires weakening artifact privacy, generated Linux output
does not reproduce byte-for-byte, or an integration resolution widens serial/write authority.
