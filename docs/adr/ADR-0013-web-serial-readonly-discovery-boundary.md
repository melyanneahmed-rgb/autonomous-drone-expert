# ADR-0013 — Web Serial read-only discovery boundary

- **Status:** Accepted for owner review — software-only transport gate
- **Date:** 2026-08-09
- **Extends:** ADR-0012 only for one exact dependency location

## Context

The WebApp needs its first production browser transport boundary without opening the M1
configuration-write lifecycle to hardware. Web Serial is asynchronous and its read chunks do
not correspond to MSP frames. Browser code must therefore execute I/O without gaining MSP
command, framing, identity, scope, or write authority.

## Decision

A dedicated `ade-web-readonly-serial-wasm-bridge` owns an explicit Rust state/effect stepper.
The canonical `ade-execution` identification implementation is extracted into
`ReadonlyIdentification` and remains the one authority used by Mock/Replay and Web Serial.
The only sequence is:

1. open the one explicitly selected port;
2. `MSP_API_VERSION`;
3. `MSP_FC_VARIANT`;
4. `MSP_FC_VERSION`;
5. `MSP_BOARD_INFO`;
6. build the typed `DeviceIdentity`, run the existing pinned scope check, and close.

The bridge accepts no `WriteApproval`, arbitrary `CommandId`, arbitrary payload, or generic
`TransportEffect`. Before exchange bytes cross the ABI, Rust verifies the transport lane,
read-only packet class, absence of approval, valid empty MSP request, exact expected command,
and current identification state. `MSP_BEEPER_CONFIG` is not an identification command and is
refused alongside every write, reboot, unknown command, malformed frame, and wrong state.

`MspV1ResponseAccumulator` in `ade-protocol-msp` owns incremental framing. It enforces reply
direction, expected command, payload length, checksum, exactly one complete frame, and an
explicit 128-byte complete-frame bound. JavaScript only returns `Uint8Array` chunks and never
parses MSP semantics.

## Browser permission and privacy

`webserial-readonly-host.mjs` is the only production browser serial authority. The owner must
invoke `selectPortFromUserGesture()`, which calls `navigator.serial.requestPort()` without an
invented VID/PID filter. There is no `getPorts()`, enumeration, automatic selection, silent
reconnection, or baud scanning. The initial scoped baud rate is an internal `115200` constant.

The selected port exists in memory only for the operation. USB serial numbers, UIDs, device
paths, COM names, permission identity, raw frames, response payloads, GPS/home data, and port
objects are neither persisted nor logged. No production React, service-worker, or UI wiring is
added in this gate.

## Narrow dependency exception

ADR-0012's exact product dependency declaration is authorised at one second and final
location for this gate:

```toml
wasm-bindgen = { version = "=0.2.127", default-features = false, features = ["std"] }
```

The accepted third-party package/version/checksum closure is identical: the root lock gains
only the new first-party bridge package and no new third-party package, version, or checksum.
The exception is machine-bound to the two exact bridge manifests. `js-sys`, `web-sys`,
`wasm-bindgen-futures`, `serde`, `serde-wasm-bindgen`, `wasm-pack`, full `wasm-bindgen-cli`,
new npm dependencies, feature/version/table drift, aliases, git sources, and wildcards remain
forbidden. The existing isolated `wasm-bindgen-cli-support 0.2.127` generator is reused.

## Error and correlation contract

Request IDs cross JavaScript as canonical decimal text. The Rust `IoCoordinator` refuses
stale, duplicate, wrong-id, and wrong-kind responses without clearing honest pending state.
Stable results distinguish in-scope identity, scope mismatch, unavailable/busy/permission or
cancelled access, disconnect, timeout, malformed response, identity/protocol failure, unknown
transport failure, and close failure. A scope match remains proposed evidence, never physical
hardware validation.

## Consequences

- The M1 `ExecutionTarget::Hardware` refusal and all write-approval rules remain unchanged.
- The storage WASM bridge remains storage-only and transport-free.
- Chrome CI uses real Rust WASM and the production host with a deterministic test-only serial
  implementation; it proves software/browser behavior, not a physical FC.
- UI integration and owner-controlled physical flight-controller testing are later gates.
- Evidence labels remain `SOFTWARE_EXERCISED`, `REAL_CHROME_EXERCISED`,
  `PHYSICAL_FC_NOT_TESTED`, and `HARDWARE_OBSERVED = NO`.
