# Web Serial read-only safety review

## Review scope

The reviewed path is `App` → prepared connection facade → `WebSerialReadonlyHost` → generated
WASM → `WasmReadonlySerialDiscovery` → `ReadonlyIdentification` →
`MspV1ResponseAccumulator` → cleanup. The review used the diagnostic child tree based on exact PR
#18 head `3555a293d7b0ac785bdd040c2e401b7d24f64fcc`. It does not claim physical FC evidence.

## Authority review

| Question | Evidence and conclusion |
| --- | --- |
| Can React choose a command or supply bytes? | No. React calls selection and zero-argument `discover()` only. Its diagnostic methods snapshot, format, clear, or record one fixed UI-boundary failure. |
| Can JavaScript substitute a directive or discovery? | No. The host imports the exact generated classes, internally constructs one discovery, and performs an `instanceof` check. Constructor/factory/binding/validator injection remains forbidden by the Python gate and browser forgeries. |
| Can a malformed directive become a write? | No. Rust decodes its own outbound packet, requires request direction, zero payload, `NoWrite`, no approval/target/recovery, the current expected command, and one of four identification commands before producing a directive. |
| Can approval be forged? | No approval crosses the WASM ABI. Any packet carrying an approval, approved target, or recovery is refused before a directive exists. |
| Can diagnostics leak a raw command? | No. Rust exports only four fixed command labels. JavaScript records labels after allowlist validation and has no command constructor or numeric command field. |
| Is there a fifth physical request? | No. Unit, static, and Chrome tests require the exact ordered IDs 1, 2, 3, and 4 and reject prohibited IDs. |

## Failure, cleanup, and retry review

Every terminal host site now has a separate fixed origin: selection, port open, writer acquisition,
reader acquisition, serial write, serial read, serial timeout, MSP frame, identity stage,
directive refusal, reader cancel/release, writer release, port close, final boundary, or UI
boundary. Raw exception content is discarded.

Normal close, read failure, parser failure, timeout cancellation, acquisition failures, release
failures, close failure, and unexpected directive paths all execute bounded cleanup. Reader and
writer locks are released when acquired, the port is closed when successfully opened, all cached
host handles are cleared, and the selected port is cleared. A demonstrated constructor-exception
edge was also closed: if the genuine WASM discovery cannot be allocated after selection, cleanup
runs before the structural `DIRECTIVE_REFUSAL` result.

Each attempt constructs a fresh Rust discovery. Port selection starts a fresh 200-event trace and
clears stage, command, and terminal origin. Real-browser tests perform two complete attempts on the
same host/port fake and prove two opens, two closes, eight total writes, and trace sequence reset.
One failed attempt therefore cannot poison the next.

The `Promise.race` timeout leaves no reusable authority: the reader is cancelled, marked, and then
released during the Rust-issued close path. The 128-chunk exchange limit and Rust accumulator size
limits prevent unbounded reads. Both the Rust protocol queue (32) and browser trace (200) evict
oldest entries deterministically.

## Parser and stream review

JavaScript never parses MSP. Rust validates framing, direction, correlation, checksum, fixed
lengths, field bounds, trailing payload, UTF-8, and final scope. The automated matrix covers whole,
split, byte-by-byte, exact-128th-chunk, trailing-byte, coalesced-frame, oversized, checksum,
wrong-command, wrong-direction, error-reply, timeout, and disconnect behavior. Timeouts and
disconnects are repeated at each of the four Rust stages. Sixty-four deterministic segmentation
seeds produce the same four stage completions and final typed identity.

## Privacy and service-worker review

Neither the host nor trace recorder logs. No trace path calls storage or network APIs. No device
metadata API (`getInfo`, VID/PID, serial number, COM path) exists in product source. The diagnostic
ring contains fixed tokens and byte counts only. Static and browser privacy attacks prove injected
device/error strings cannot reach events, UI, copy, or results.

The service worker cannot access the page heap or Web Serial object. On the PR #18 child tree its
root-absolute asset strategy is not suitable for repository-scoped Pages; current `main` already
contains the versioned, base-path-aware worker from PR #19/#20. That is an integration conflict,
not permission for the diagnostic branch to replace the accepted worker overnight.

## Findings fixed in the child branch

1. Context-free `Unknown` terminal results now retain a fixed `failureOrigin`.
2. Writer/reader acquisition and every cleanup operation now have distinct safe origins.
3. Rust now supplies command, stage, frame decision, direction, and parser reason without raw data.
4. The selected port is cleaned if discovery construction itself fails.
5. Trace state is bounded, immutable to consumers, per-attempt, copy-sanitized, and clearable.
6. The expanded matrix proves retry and adversarial stream behavior rather than only the happy
   path.

## Residual boundaries

- No physical FC test ran. Hardware support and identity completion remain unvalidated.
- PR #18 and current `main` are divergent and must be integrated under the separate plan before
  either PR #18 or this child can target `main` safely.
- Generated `wasm-bindgen` glue contains its upstream initialization/MIME fallback warnings. They
  contain no serial/device/frame data and are not used by the diagnostic recorder, but a future
  generator review may choose to suppress them without hand-editing derived output.

No critical Web Serial safety blocker was found after the fixes above.
