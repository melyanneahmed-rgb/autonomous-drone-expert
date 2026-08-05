# M1A — Windows serial transport spike: comparison report

**Status: spike. Not production code. Must never be merged into `main`.**
Verified 2026-08-05 against sources pinned in `docs/DEPENDENCY-AUDIT.md`.

## The question

Which Rust serial layer should the production transport crate be built on, Windows first?

## The short answer

**No single library wins outright, and the deciding evidence cannot be produced without
hardware.** The two candidates fail in opposite directions:

- `serialport` is the only one that can tell us **which device** it found. It is also the
  one with a copyleft licence, a native Linux dependency that breaks builds, an older
  Windows binding, and a maintainer shortage on our primary platform.
- `serial2` is cleaner in every structural way — permissive licence, no native code,
  current bindings, most recent activity, better error fidelity, better threading
  primitive — and it **cannot identify a device at all**, which breaks reconnect matching.

## Evidence table

| Dimension | `serialport` 4.9.0 | `serial2` 0.2.38 | Evidence |
| --- | --- | --- | --- |
| Enumeration metadata | VID, PID, serial number, manufacturer, product | **Port names only** | `CI_VERIFIED` |
| Reconnect identity after COM renumber | Possible | **Impossible without another source** | `CI_VERIFIED` (model) |
| Error fidelity | Own error type; `NoDevice` merges "absent" and "in use"; raw OS code recoverable | Plain `std::io::Error`, `raw_os_error()` intact | `CI_VERIFIED` |
| Licence | MPL-2.0 (file-level copyleft) | BSD-2-Clause OR Apache-2.0 | Verified from crates.io |
| Native C dependency | **Yes on Linux by default** (`libudev-sys`) | None anywhere | `CI_VERIFIED` — build failure reproduced |
| Windows bindings | `windows-sys 0.52` | `windows-sys 0.61` | Verified from crates.io |
| Declared MSRV | 1.59.0 | 1.63 | Verified; both suit 1.85.0 |
| Last commit observed | 2026-07-25 | 2026-07-31 | Verified from repositories |
| Maintainer signal | Actively committed, **publicly seeking maintainers, especially Windows** | Active, single maintainer | Verified |
| Open Windows issues | 6, incl. enumeration failure on Windows 11 and truncated serial numbers | `UNVERIFIED` | Verified from issue tracker |
| COM above 9 | Handled (`\\.\`) | Handled (`\\.\`) | Source inspected at pinned versions |
| Forces `unsafe` in our code | No | No | `CI_VERIFIED` |
| Cancellation primitive | **None** | **None** | `CI_VERIFIED` (API inspection) |
| Concurrent use from threads | `try_clone` | `&self` read/write, documented concurrent on Windows | Verified from docs and source |
| Timeout model | Single `timeout()` for read and write | Separate read and write timeouts | Verified |

## What the CI runner actually proved

GitHub-hosted Windows runners expose **no serial hardware**, so anything requiring an open
port is out of reach. Every assertion in this spike is labelled with what it proves.

**`CI_VERIFIED`** — executed on the runner, real behaviour:
enumeration returns without hanging and without panicking, on both backends, repeatedly;
absent ports, invalid names, empty names and COM numbers above 9 all produce a
**classified** error rather than a panic, a hang or `UNKNOWN_TRANSPORT_ERROR`; 25
consecutive failed opens show no degradation; metadata capability differs exactly as
documented; the reconnect model behaves correctly on synthetic snapshots; neither library
forces `unsafe` into our code.

**`SIMULATED_ONLY`** — logic exercised with synthetic input:
the Windows error-code mapping over documented codes; the deadline and cooperative-cancel
architecture; the watchdog reporting a hung call as a failure rather than as slowness.

**`REQUIRES_WINDOWS_HARDWARE_TEST`** — cannot be reached here at all:
read and write timeout accuracy; `PORT_BUSY` from a second client; whether dropping a
handle unblocks a read in progress; unplug during a read; replug detection; COM
renumbering; handle release after process kill; driver-absent behaviour.

`serial2::SerialPort::pair()` would have allowed real I/O tests in CI, but it is
**Unix-only**. There is no way to close this gap on a hosted Windows runner short of
installing a virtual serial driver, which needs administrator rights and an unsigned
driver — not something to do on a hosted runner. A self-hosted Windows runner with a
USB adapter attached would close it properly.

## Cancellation and timeouts

Neither candidate can be cancelled. There is no `cancel`, no shutdown, no interrupt. The
only portable tools are a bounded read timeout and dropping the handle.

The consequence for the production design is concrete: **every read must be bounded**, a
cancellation flag must be checked between reads, and the session layer must own the
thread so that a stuck device costs one thread rather than the interface. The spike
implements and tests exactly that shape in `src/watchdog.rs` and `tests/cancellation.rs`.

Whether dropping a handle actually unblocks an in-flight `ReadFile` on Windows is the
single most important open question, and it is test 8 in the manual hardware plan.

`serial2` is the better substrate for this design: `read` and `write` take `&self` and the
crate documents concurrent use from multiple threads on Windows, so a second handle can be
held by the canceller. `serialport` offers `try_clone` but no documented concurrency
guarantee of the same strength.

## Enumeration and metadata — the decisive difference

`serialport` returns `SerialPortInfo` with USB descriptors. `serial2` returns
`Vec<PathBuf>`.

This is not a convenience gap. The product must survive a board that reboots into
bootloader and comes back on a different COM number, and must never write to the wrong
device when two boards are attached. `tests/reconnect_model.rs` demonstrates both
outcomes: with metadata, a renumber is reported as a **rename**; without it, the same
event is indistinguishable from an unplug followed by an unrelated plug.

If `serial2` were chosen, we would have to write our own Windows device enumeration
(SetupAPI or WMI) to recover VID, PID and serial number. That is our code, our `unsafe` or
our binding dependency, and its own audit — a real cost that must be priced into the
decision rather than discovered later.

## Error model

Twelve variants are proposed (`src/error.rs`). Eleven are reachable from documented
Windows codes. `PERMISSION_DENIED` is Unix-leaning: on a Windows COM port,
`ERROR_ACCESS_DENIED` means the port is held by someone else, which we map to `PORT_BUSY`
because that is the diagnosis a user can act on.

Neither library produces our vocabulary. **The mapping adapter is ours either way** —
which also means the error model is not a reason to choose one library over the other,
except that `serial2` preserves the raw OS code more directly.

## Risks

1. **`serialport` maintainer shortage on Windows** is the highest strategic risk: it is
   our primary platform and the crate's stated weak spot.
2. **MPL-2.0** keeps a licensing question open while the product licence is still
   deferred.
3. **Choosing `serial2` means owning Windows device enumeration ourselves** — new code in
   the most platform-specific area we have, and the one place a mistake means writing to
   the wrong aircraft.
4. **No cancellation in either** — the design must absorb this; it cannot be bought.
5. **`libudev` build failure** would break any Linux developer or CI job that forgets
   `default-features = false`.
6. **Single-maintainer risk on `serial2`.**
7. **Windows ARM64 is `UNVERIFIED` for both.**

## Recommendation

### `NO DECISION — HARDWARE TEST REQUIRED`

This is the honest outcome, not a deferral. The two candidates are separated by exactly
the properties CI cannot measure: cancellation behaviour on a real handle, `PORT_BUSY`
against a real second client, and replug identity across a real COM renumber.

Provisional standings, to be confirmed or overturned by the manual hardware plan:

- **Leading candidate: `serialport` 4.9.0** — solely because device metadata is
  non-negotiable for reconnect safety, and it is the only candidate that provides it
  today. Conditional on `default-features = false`, an MPL-2.0 review before distribution,
  and hardware results.
- **Backup candidate: `serial2` 0.2.38** — structurally the better dependency in every
  other respect. It becomes the leader if hardware testing shows that dropping a handle
  cancels a read cleanly on `serial2` and not on `serialport`, or if we decide to own
  Windows enumeration regardless.
- **`tokio-serial`: rejected** as an independent candidate — it depends on `serialport`.

A third path deserves evaluation before M1B closes: **`serial2` for I/O plus our own
Windows enumeration**, which would give permissive licensing, no native dependencies, the
better threading primitive, and full metadata — at the cost of code we must write and
audit ourselves.

## What must not be concluded from this spike

- That either library works with a flight controller. **No hardware has been connected.**
- That timeouts are accurate. Not measured.
- That reconnect works. Only the matching model was tested, on synthetic data.
- That a dependency has been approved for production. None has.
