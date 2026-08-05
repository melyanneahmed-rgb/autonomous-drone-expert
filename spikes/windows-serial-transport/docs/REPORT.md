# M1A — Windows serial transport spike: comparison report

**Status: spike. Not production code. Must never be merged into `main`.**
Verified 2026-08-05 against sources pinned in `docs/DEPENDENCY-AUDIT.md`.
Amended after owner review: identity model corrected, architectures widened to three,
cancellation claims narrowed to what was actually proven, audit tools pinned.

## The question

Which serial architecture should the production transport be built on, Windows first —
not merely which library?

## The three architectures compared

| | Discovery + identity | Byte I/O | Note |
| --- | --- | --- | --- |
| **A** | `serialport` | `serialport` | One crate for everything |
| **B** | `serialport` (metadata) | `serial2` (I/O) | Hybrid; wiring proven to compose in CI (`tests/discovery_probe.rs`) |
| **C** | Independent USB layer (`nusb` + a SetupAPI/WMI join) | `serial2` | Permissive licences throughout; the COM join becomes our code |

`tokio-serial` remains rejected as an independent candidate: it depends on
`serialport ^4` (as does `mio-serial`), so it is architecture A behind an async facade.

## Evidence table (libraries)

| Dimension | `serialport` 4.9.0 | `serial2` 0.2.38 | Evidence |
| --- | --- | --- | --- |
| Enumeration metadata | VID, PID, serial, manufacturer, product | **Port names only** | `CI_VERIFIED` |
| Error fidelity | Own type; `NoDevice` merges "absent" and "in use"; raw OS code recoverable | Plain `io::Error`, `raw_os_error()` intact | `CI_VERIFIED` |
| Licence | MPL-2.0 (file-level copyleft) | BSD-2-Clause OR Apache-2.0 | crates.io |
| Native C dependency | **Yes on Linux by default** (`libudev-sys`; build failure reproduced) | None | `CI_VERIFIED` |
| Windows bindings | `windows-sys 0.52` | `windows-sys 0.61` | crates.io |
| MSRV / suits 1.85.0 | 1.59.0 / yes | 1.63 / yes | crates.io |
| Last commit observed | 2026-07-25 | 2026-07-31 | repositories |
| Maintainer signal | Active; **publicly seeking maintainers, especially Windows** | Active; single maintainer | repositories |
| COM > 9 (`\\.\` namespace) | Handled | Handled | pinned source |
| Forces `unsafe` in our code | No | No | `CI_VERIFIED` |
| Cancellation primitive | **None** | **None** | API inspection |
| Threading primitive | `try_clone` | `&self` read/write, documented concurrent on Windows | docs + source |
| Timeout model | One `timeout()` for both directions | Separate read and write timeouts | source |

## Path C: can an independent layer bind USB identity to a COM name?

`nusb` 0.2.5 was prototyped (`src/discovery.rs`, `tests/discovery_probe.rs`):

- **What it provides** (source-verified): VID, PID, manufacturer/product/serial strings
  cross-platform; on Windows additionally `instance_id`, `parent_instance_id`,
  `location_paths`, `port_chain`, `driver`; hotplug via `watch_devices()`. Pure Rust, no
  libusb, Apache-2.0 OR MIT, no `unsafe` needed in our code. Enumeration ran bounded and
  tolerant on both CI runners (`CI_VERIFIED`).
- **What it cannot do**: **no API returns a COM port name** — the strings `PortName` and
  `GUID_DEVINTERFACE_COMPORT` do not occur anywhere in its source. The finding is encoded
  as an executable test, not prose.
- **The join that remains ours**: USB instance ID (which nusb exposes) → child device
  node of the serial function → `PortName` in the device registry, via
  SetupAPI/CfgMgr32. Implementing it requires either direct FFI (`windows-sys`/`windows`
  — `unsafe` calls in our code, its own review) or WMI queries via the `wmi` crate (no
  `unsafe` in our code; adds a COM/WMI runtime dependency and string parsing). Audit rows
  for all three are in `docs/DEPENDENCY-AUDIT.md`.
- **End-to-end demonstration on a real adapter: `REQUIRES_WINDOWS_HARDWARE_TEST`** (added
  to the manual plan). The hosted runner exposes no USB serial device.

Answer to the direct question: **nusb alone — no. nusb plus a SetupAPI/registry join —
documented-feasible via the instance-ID key, not yet demonstrated.**

## Device identity — corrected model

The earlier revision of this spike wrongly treated VID/PID + descriptor strings as
device identity and auto-renamed on that basis. Corrected (`src/reconnect.rs`, mandated
tests in `tests/reconnect_model.rs`, all `CI_VERIFIED` on synthetic data):

- Only a **non-empty, matching serial number** yields `UNIQUE_IDENTITY_MATCH`.
- Present-but-different serials are `NO_MATCH`.
- VID/PID + manufacturer/product yield at most `POSSIBLE_MATCH`; **no automatic rename**
  below unique — look-alikes are surfaced as diagnostics only.
- A bare COM name is **session continuity only**; after a disappearance it carries no
  identity.
- Multiple candidates (including colliding serials, which cheap clones ship) are
  `AMBIGUOUS_DEVICE_IDENTITY` and block any future write until re-identification.
- **No resolution authorises writes from OS metadata.** Every path leaves writes blocked
  until a read-only firmware identity handshake (documented contract; no MSP exists in
  this spike). `WritePolicy` has no permissive variant by construction.

Consequence for the comparison: `serialport`'s metadata advantage is real but **narrower
than first stated** — it enables *unique* matching only when the device reports a serial
number, and several of its open Windows issues are precisely about missing or truncated
serial numbers. General metadata alone is therefore **not a sufficient reason to choose a
library**, and the earlier "provisional lead on metadata alone" is withdrawn.

## Cancellation — stated honestly

- The cooperative cancellation demonstrated here is **`SIMULATED_ONLY`**: it cancels a
  synthetic loop, not a blocked `ReadFile`.
- The watchdog **does not stop a hung thread; it only stops waiting for it.** The thread
  keeps running.
- Whether dropping a handle interrupts a real blocking read on Windows is
  **`REQUIRES_WINDOWS_HARDWARE_TEST`**, for both libraries. Handle drop is a *candidate*
  mechanism, not a proven interrupt.
- **No candidate has a proven cancellation advantage.** `serial2`'s `&self` I/O and
  documented cross-thread concurrency make it the more convenient substrate *if* the
  mechanism proves out — that is a design convenience, not a measured result.

## Complete-write semantics

The contract now separates `write_some` (may accept any prefix, including zero) from
`write_all_with_deadline` (success **only** at full length, failure reports
`bytes_written`). Simulation tests (`SIMULATED_ONLY`, injected clock, no device): partial
then completion; multiple partials with a zero-progress step; zero-byte payload without
touching the transport; timeout before completion; disconnect mid-write; and a sweep
asserting success is never declared below full length.

## What each label covers now

**`CI_VERIFIED`** — enumeration bounded/tolerant on both serial backends and on nusb;
absent/invalid/empty/COM>9 opens fail classified on both; 25 failed opens with no
degradation; metadata capability difference; identity rules on synthetic snapshots;
architecture-B wiring composes; neither library forces `unsafe`.

**`SIMULATED_ONLY`** — Windows error-code mapping; cooperative cancellation pattern;
watchdog semantics; all complete-write tests.

**`REQUIRES_WINDOWS_HARDWARE_TEST`** — timeout accuracy; real `PORT_BUSY`; handle drop
during a blocked read; unplug/replug; COM renumbering; process-kill handle release;
driver-absent behaviour; USB→COM join end-to-end; identity flow against real twin
devices; ARM64 for everything.

## Risks (updated)

1. `serialport` maintainer shortage on Windows — our primary platform.
2. MPL-2.0 while the product licence is deferred.
3. Path C means owning the USB→COM join — `unsafe` FFI or a WMI dependency, in the one
   place a mistake targets the wrong aircraft.
4. Serial numbers are not guaranteed: absent or truncated serials collapse identity to
   `POSSIBLE_MATCH`, whichever library enumerates.
5. No cancellation primitive anywhere; the design must absorb it.
6. `libudev` default-feature build failure on Linux.
7. Single-maintainer risk on `serial2`; Windows ARM64 `UNVERIFIED` for all candidates.

## Recommendation

### `NO DECISION — HARDWARE TEST REQUIRED`

Unchanged in verdict, updated in reasoning. The deciding evidence is exactly what CI
cannot produce: handle-drop behaviour during a real read (per library), real `PORT_BUSY`,
replug identity with and without serial numbers, and the USB→COM join on a physical
adapter. Metadata alone is not grounds for selection, and no cancellation advantage is
proven.

Standing of the three architectures going into the hardware phase:

- **A (`serialport` everywhere)** — viable; carries the licence, maintenance and
  default-feature liabilities; strongest today only where devices actually report serial
  numbers.
- **B (hybrid)** — wiring proven to compose; inherits A's enumeration liabilities while
  adding a second dependency; justified only if hardware shows `serial2`'s I/O behaviour
  is materially better.
- **C (`serial2` + independent discovery)** — cleanest licences and I/O substrate; the
  USB→COM join is documented-feasible via nusb's instance ID but is unwritten,
  unproven code we would own and audit.

The manual hardware plan (now 16 tests) decides. No production dependency is approved by
this report.

## Hardware results so far

Partial M1B hardware results are recorded in `M1B-RESULTS.md`
(`M1B HARDWARE VALIDATION — PARTIAL PASS, PAUSED BY OWNER`,
`NO BACKEND DECISION — REMAINING HARDWARE TESTS REQUIRED`). Notably: a `PORT_BUSY`
classification divergence under contention (serialport reported `PORT_NOT_FOUND`,
serial2 reported `PORT_BUSY`), comparable read-timeout accuracy, and a
clone-handle diagnostic on serialport where dropping the original did not cancel
the clone's in-flight read. serial2 clone-handle, replug/renumber and process-kill
remain pending.

## What must not be concluded from this spike

- That either library works with a flight controller. No hardware has been connected.
- That any cancellation mechanism works. None is proven.
- That reconnect identity works end-to-end. Only the matching rules are proven, on
  synthetic data; the firmware handshake is a documented contract, not code.
- That a dependency has been approved for production. None has.
