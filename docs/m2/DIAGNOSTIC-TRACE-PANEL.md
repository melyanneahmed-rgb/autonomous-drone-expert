# Temporary read-only diagnostic trace

## Purpose and evidence boundary

This panel observes one existing read-only FC identification attempt. It does not add a command,
transport effect, approval, device selector, or hardware claim. The exact physical command set
remains the four Rust-owned empty-payload reads: `MSP_API_VERSION`, `MSP_FC_VARIANT`,
`MSP_FC_VERSION`, and `MSP_BOARD_INFO`. `discover()` remains zero-argument and
`hardwareObserved` remains `false`.

The panel is temporary owner/developer observability for the next explicitly approved physical
USB-only attempt. Software and fake-device browser tests are not physical evidence.

## Architecture

The event path is intentionally split by authority:

1. `WebSerialReadonlyHost` records browser operation boundaries, safe byte counts, fixed browser
   failure classes, cleanup, and structural origins.
2. `WasmReadonlySerialDiscovery` remains authoritative for the identification stage, command,
   MSP frame acceptance/rejection, direction, and parser reason.
3. Rust exposes those facts only through a bounded `WasmReadonlyTraceEvent` and destructive
   `takeTraceEvent()` queue read. It exposes no frame or payload.
4. `DiagnosticTraceRecorder` validates every field against fixed allowlists and stores frozen
   events in a per-attempt RAM ring.
5. React receives an immutable snapshot. Its collapsed `<details>` panel renders newest first so
   the terminal event is immediately visible. Copy uses a separately validated fixed-format text
   representation; clear empties the RAM ring only.

The Rust queue is capped at 32 protocol events. The browser trace is capped at 200 events, within
the owner-approved 100–250 range. Two hundred retains the complete four-read journey even when
accepted test replies arrive byte-by-byte. At capacity, the oldest event is dropped
deterministically and sequence numbers remain monotonic for the attempt. A new port-selection
attempt clears the prior trace and restarts sequence numbering at one.

## Complete vocabulary

Layers:

`UI`, `HOST`, `RUST`, `SERIAL`, `MSP`, `CLEANUP`

Phases:

`PORT_SELECTION`, `DISCOVERY`, `PORT_OPEN`, `API_VERSION`, `FC_VARIANT`, `FC_VERSION`,
`BOARD_INFO`, `SERIAL_WRITE`, `SERIAL_READ`, `MSP_FRAME`, `IDENTITY_STAGE`, `PORT_CLOSE`,
`CLEANUP`, `UI_BOUNDARY`, `FINAL_RESULT`

Events:

`SELECT_START`, `SELECT_OK`, `SELECT_FAILED`, `DISCOVERY_START`, `PORT_OPEN_START`,
`PORT_OPEN_OK`, `PORT_OPEN_FAILED`, `DIRECTIVE`, `TX_START`, `TX_OK`, `TX_FAILED`, `RX_CHUNK`,
`RX_FAILED`, `FRAME_ACCEPTED`, `FRAME_REJECTED`, `IDENTITY_STAGE_OK`,
`IDENTITY_STAGE_FAILED`, `PORT_CLOSE_START`, `PORT_CLOSE_OK`, `PORT_CLOSE_FAILED`,
`CLEANUP_START`, `CLEANUP_OK`, `CLEANUP_FAILED`, `UI_BOUNDARY_FAILED`, `FINAL_OK`,
`FINAL_FAILED`

Origins:

`PORT_SELECTION`, `DISCOVERY`, `PORT_OPEN`, `WRITER_ACQUISITION`, `READER_ACQUISITION`,
`SERIAL_WRITE`, `SERIAL_READ`, `SERIAL_TIMEOUT`, `MSP_FRAME`, `IDENTITY_STAGE`,
`DIRECTIVE_REFUSAL`, `PORT_CLOSE`, `READER_CANCEL`, `READER_RELEASE`, `WRITER_RELEASE`,
`CLEANUP`, `UI_BOUNDARY`, `FINAL_RESULT`

Directions:

`REQUEST`, `REPLY`, `ERROR`

Failure classes:

`Unavailable`, `Cancelled`, `PermissionDenied`, `PortBusy`, `Disconnected`, `Timeout`,
`MalformedResponse`, `ProtocolIdentityFailure`, `HardwareEvidenceBoundary`, `CloseFailure`,
`Unknown`

Parser reasons:

`PayloadTooLong`, `FrameTooLarge`, `Truncated`, `TrailingBytes`, `BadPreamble`, `BadDirection`,
`BadChecksum`, `WrongLength`, `WrongCommand`, `WrongDirection`, `ErrorReply`,
`ReplyMisclassified`, `FieldOverrun`, `TrailingPayload`, `InvalidUtf8`,
`OtherProtocolIdentityFailure`

## Privacy and non-persistence contract

An event has exactly these possible fields: sequence, layer, phase, event, stage, command,
byteCount, direction, failureClass, failureReason, and origin. There is no metadata map, arbitrary
string, browser exception, `details`, generic logger, or formatter for caller-provided data.

The recorder rejects unknown keys, out-of-vocabulary strings, non-integer or over-limit byte
counts, and failure events without a fixed origin. It never receives or stores TX bytes, RX bytes,
payloads, serial/USB metadata, identity values, error messages, stacks, or arbitrary objects.

Trace events are not written to IndexedDB, localStorage, sessionStorage, cookies, casebook,
CacheStorage, GitHub, analytics, a beacon, or a network endpoint. The service worker may cache the
static application code like any other application bundle; it cannot access or cache the page's
in-memory event objects. No diagnostic code calls a console API.

The privacy-attack regression injects `COM99`, `SERIAL-SECRET-123`, `VID_1234`, `PID_ABCD`,
`/private/path`, and `raw-device-name` through fake port and browser error objects. The test proves
that none reaches events, the UI, copied trace, or terminal fields.

## What the owner will see

After an attempt, the existing FC connection card contains a collapsed temporary diagnostic
section. Expanding it shows numbered fixed-token events in monospace with the terminal event at
the top. A host exception may still classify as `Unknown`, but its origin will be explicit, for
example `origin=PORT_OPEN` or `origin=UI_BOUNDARY`. A protocol failure shows the Rust-owned stage,
frame decision, parser reason, close/cleanup result, and terminal result without showing the
response itself.

`نسخ السجل الآمن` copies only `FPV_ARBCON_READONLY_DIAGNOSTIC_TRACE_V1` and the validated event
lines. `مسح` clears only the current page's RAM trace.

## Automated evidence

- Rust bridge: 12 unit tests, including all-stage command/direction/error rejection, exact stage
  diagnostics, malformed BOARD_INFO cases, fixed vocabulary, and 64 deterministic randomized
  segmentation seeds.
- Trace recorder: four unit tests for freezing, 200-event eviction, per-attempt reset, fixed copy
  formatting, malicious field rejection, and absence of logging/persistence/network sinks.
- Real Chrome Rust/WASM host gate: scenario groups A–J, covering selection, open/acquisition,
  serial I/O, parser, chunk boundaries, timeouts/disconnects at every stage, cleanup failures,
  repeat attempts, privacy injection, exact four reads, and zero-argument discovery.
- Real Chrome production gate: in-scope, scope mismatch, typed BOARD_INFO failure, cancellation,
  and API-unavailable paths; it exercises collapsed state, copy, clear, and UI privacy.

No new npm or Rust dependency is used.
