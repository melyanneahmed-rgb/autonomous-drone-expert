# M2 — Product Core and WebAssembly Boundary

**Status:** in progress — Commit 1 boundary, no hardware contact

M2 turns the accepted M1 safety core into a product-level core that can be driven by a Web/PWA
shell without moving protocol, write authority or recovery decisions into TypeScript. M2 is a
core/UI architecture milestone. It does not open a serial port, identify a flight controller
or write to hardware.

## Commit 1 — implemented boundary

### Generic product plan

`ade-planning::product` now represents a configuration plan as ordered setting deltas:

- a pack-owned numeric setting id inside one of the fourteen product domains;
- protocol-independent before and after values;
- the decision source;
- a declared recovery class;
- one or more pinned provenance record ids.

Construction rejects unchanged values, duplicate settings, missing provenance and unusable
recovery declarations. The binding responsibility rule is structural:

- the thirteen automatic domains accept only `ProgramDerived` changes;
- `ControlFunctionAssignments` accepts only a typed value identical to the user's confirmed
  control assignment; a different numeric or structured value is rejected;
- neither representation carries a protocol command, a UART selection or write approval.

An empty `ProductConfigurationPlan` is the single explicit no-op form.

### Asynchronous host-I/O boundary

`ade-runtime-ports` performs no I/O. The deterministic core emits a typed `IoEffect`; a browser
or native host completes it later with the same `RequestId` and response kind.

- Transport and storage have independent lanes, with at most one effect in flight per lane.
- Stale ids, duplicated responses and wrong response kinds are refused without clearing the
  pending request.
- Read packets carry no write authority.
- Write/reboot packets can be constructed only from an existing `WriteApproval`. Because M1
  cannot produce a hardware `WriteApproval`, this boundary cannot make a hardware write
  representable.
- Storage keys are bounded identifiers, not paths. Compare-and-swap commits carry an expected
  revision so IndexedDB transactions or native atomic replacement report conflicts instead of
  overwriting a newer case record.
- Debug representations redact storage keys and all raw transport/storage bytes; they expose
  only request metadata, failure classes and payload lengths.

The host adapter owns the selected port handle and the IndexedDB/native storage handle. The
ordinary UI consumes product contracts only; it does not receive protocol frames or storage
bytes.

## WebAssembly proof

CI cross-compiles `ade-planning` and `ade-runtime-ports` for `wasm32-unknown-unknown` using the
pinned Rust toolchain. This proves the first deterministic boundary is target-compatible. It
does **not** claim that Web Serial, IndexedDB or a flight controller has been integrated.

## Safety and evidence boundary

- No new external dependency.
- No serial, USB, Web Serial, filesystem, IndexedDB or network implementation.
- No MSP/CLI command or provenance fact added.
- No flight controller contacted and no hardware-support claim.
- M1 Mock/Replay behaviour and its write gates remain unchanged.

## Remaining M2 work

1. Drive the existing M1 lifecycle through host effects rather than direct synchronous I/O,
   while retaining the synchronous Mock/Replay adapter for regression tests.
2. Split journal encoding/reconciliation from native file persistence and add a storage-effect
   adapter contract for browser crash-safe resume.
3. Add the repository-owned PWA shell, offline cache tests and product-contract bindings.
4. Add UI interaction tests for the product input flow and manual control-assignment boundary.
5. Complete workspace review and acceptance before M3 read-only Web Serial identification.

Hardware reads begin only in M3. Hardware writes remain blocked until M4 and require a separate
owner approval even if all M2 tests pass.
