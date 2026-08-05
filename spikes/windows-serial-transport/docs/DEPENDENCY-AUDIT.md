# Dependency audit — M1A candidates

All figures were read from crates.io and the official repositories on **2026-08-05**.
Nothing is quoted from memory. Anything not verified is marked `UNVERIFIED`.

**No candidate is adopted by this document.** Every decision below is a spike-scope
decision. Bringing any crate into a production crate requires its own reviewed pull
request.

---

## Candidate A — `serialport`

| Field | Value |
| --- | --- |
| Version pinned in the spike | `=4.9.0`, `default-features = false` |
| Release date | 2026-03-16 |
| Licence | **MPL-2.0** |
| Declared MSRV | 1.59.0 |
| Repository | `serialport/serialport-rs` |
| Last commit observed | 2026-07-25 |
| Downloads (all versions) | ~16.9 M |
| Direct dependencies | `cfg-if`, `scopeguard`; Windows: `windows-sys ^0.52`; Unix: `bitflags`, `nix ^0.26`; Linux (non-musl): `unescaper`, `libudev` (optional, **on by default**); `serde` (optional) |
| Native (C/C++) dependencies | **Yes on Linux by default** — `libudev-sys` links the system `libudev`. **None on Windows**, which uses `windows-sys` bindings. |
| Extra DLLs at runtime on Windows | None observed |
| `unsafe` in our code | Not required — the spike compiles under `#![forbid(unsafe_code)]` |
| `unsafe` inside the crate | 95 occurrences across `src/` (expected for FFI; informational) |
| COM numbers above 9 | Handled — applies the `\\.\` device namespace (`src/windows/com.rs`) |
| Windows x64 | Supported |
| Windows ARM64 | `UNVERIFIED` |
| Suits Rust 1.85.0 | Yes — declared MSRV is far below |
| Open Windows issues | 6, clustering on enumeration failures (#310, Windows 11), serial-number and interface metadata (#351, #203), and timeout configurability (#275) |
| Maintenance signal | Actively committed, **but the project publicly asks for maintainers, especially for Windows**, and its `windows-sys` pin (0.52) trails the current line |

### Findings

1. **Hard build failure on Linux with default features.** `libudev-sys`'s build script
   panics on any machine without libudev development files:
   *"The system library `libudev` required by crate `libudev-sys` was not found."*
   This was hit in the spike container and is reproducible. `default-features = false`
   fixes the build at the cost of degraded Linux enumeration metadata. Windows is
   unaffected. A Windows-first project can absorb this; a Linux CI job cannot ignore it.
2. **Richest metadata of any candidate** — `SerialPortInfo` carries VID, PID, serial
   number, manufacturer and product. This is the only candidate that can support device
   identity across a reconnect.
3. **Licence is MPL-2.0**, file-level copyleft. Linking is not the concern; modifying its
   files is. The production licence decision is still deferred, so this must be revisited
   before any distribution.

**Decision (spike scope): `ACCEPT WITH CONDITIONS`** — conditions: `default-features =
false` on Linux, a legal review of MPL-2.0 before distribution, and hardware validation
of the Windows behaviours listed in the manual test plan.

---

## Candidate B — `serial2`

| Field | Value |
| --- | --- |
| Version pinned in the spike | `=0.2.38` |
| Release date | 2026-07-31 |
| Licence | **BSD-2-Clause OR Apache-2.0** |
| Declared MSRV | 1.63 |
| Repository | `de-vri-es/serial2-rs` |
| Last commit observed | 2026-07-31 |
| Downloads (all versions) | ~7.0 M |
| Direct dependencies | Windows: `windows-sys ^0.61`; Unix: `cfg-if`, `libc`; `serde` (optional) |
| Native (C/C++) dependencies | **None on any platform** |
| Extra DLLs at runtime on Windows | None observed |
| `unsafe` in our code | Not required |
| `unsafe` inside the crate | 49 occurrences across `src/` (informational) |
| COM numbers above 9 | Handled — applies the `\\.\` device namespace (`src/sys/windows/mod.rs`) |
| Windows x64 | Supported |
| Windows ARM64 | `UNVERIFIED` |
| Suits Rust 1.85.0 | Yes |
| Open Windows issues | `UNVERIFIED` — not enumerated in this batch |
| Maintenance signal | Most recently released and committed of all candidates; single-maintainer project, which is its own risk |

### Findings

1. **Smallest and cleanest dependency surface** — no native C anywhere, current
   `windows-sys`, and a dual permissive licence that raises no distribution question.
2. **Enumeration returns paths only.** `available_ports() -> io::Result<Vec<PathBuf>>`.
   No VID, PID, serial number, manufacturer or product. This is the decisive limitation:
   without metadata, a COM renumber is indistinguishable from unplug-plus-plug, which the
   spike demonstrates in `tests/reconnect_model.rs`.
3. **Errors are plain `std::io::Error`**, so `raw_os_error()` survives and our mapping is
   direct. `serialport` wraps errors in its own type and collapses "absent" and "in use"
   into `ErrorKind::NoDevice`, which the raw code has to rescue.
4. **`read`/`write` take `&self`** and the crate documents concurrent use from multiple
   threads, including on Windows. This is the better primitive for a future cancellation
   design, even though no cancel API exists.
5. `pair()` — a connected pseudo-terminal pair, which would have enabled real I/O tests in
   CI — is **Unix-only**. It cannot rescue Windows CI coverage.

**Decision (spike scope): `ACCEPT WITH CONDITIONS`** — conditions: enumeration metadata
must be obtained another way (a separate Windows device-enumeration path, likely via
SetupAPI or WMI, which becomes our own code and its own audit), plus the same hardware
validation.

---

## Candidate C — `tokio-serial` (evaluated, not implemented)

| Field | Value |
| --- | --- |
| Version | `5.5.0`, released 2026-06-15 |
| Licence | MIT |
| Direct dependencies | `mio-serial ^5.0.3`, **`serialport ^4`**, `tokio`, `futures-core`, `futures-sink`, `log`, `cfg-if` |
| Last commit observed | `UNVERIFIED` (release date 2026-06-15) |

### Finding that ends the evaluation

`tokio-serial` **depends on `serialport ^4`**, and so does `mio-serial`. It is not an
independent alternative — it is an async wrapper around Candidate A. Choosing it means
accepting `serialport`'s MPL-2.0 licence, its `libudev` behaviour and its maintenance
risk, **plus** a tokio runtime commitment and two extra layers of maintainer.

It was therefore not implemented as a backend. Doing so would have measured the same
underlying library twice and presented the result as a comparison.

**Decision (spike scope): `REJECT` as an independent candidate.** It remains an option
only if `serialport` is chosen *and* an async facade is later wanted — and even then,
wrapping our own contract is likely cheaper than inheriting theirs.

---

## `serial2-tokio` (noted, not implemented)

`0.1.25`, same licence and maintainer as `serial2`, released 2026-07-31, depends on
`serial2` plus `tokio`. It is a thin async layer over the crate the spike does test, so
the blocking findings above transfer. Declared MSRV: none. Not implemented, because the
contract chosen for this spike is blocking (see `TRANSPORT-CONTRACT.md`).

---

## Cross-cutting observations

- **Duplicate `windows-sys`.** The resolved lock contains `windows-sys 0.52.0`
  (via `serialport`) and `0.61.2` (via `serial2`). Harmless in a spike; if both crates
  ever shipped together in production it would be a duplicate-version finding.
- **Neither crate forced `unsafe` into our code.** The spike compiles under
  `#![forbid(unsafe_code)]` against both.
- **Neither crate offers cancellation.** No `cancel`, no shutdown, no interrupt in either
  public API. This is a property of the problem, not of the libraries.

## Tooling results

Run against this spike's own `deny.toml`, which allows MPL-2.0 **for the spike only**. The
production allowlist at the repository root is unchanged and still does not permit
MPL-2.0.

| Tool | Version | Result |
| --- | --- | --- |
| `cargo-deny` | 0.20.2 | `advisories ok, bans ok, licenses ok, sources ok` |
| `cargo-audit` | latest at run time | 32 crate dependencies scanned, **no vulnerabilities** |

Two notes recorded rather than suppressed:

- `cargo-deny` initially failed `licenses` on **our own package**, because the project is
  deliberately unlicensed while the licence decision is deferred. Resolved with
  `[licenses.private] ignore = true`, which applies only to workspace members marked
  `publish = false` and relaxes nothing for third-party code.
- `bans` reports duplicate versions of `bitflags` (1.3.2 via `nix` via `serialport`, and
  2.13.1 via `serialport`) and of `windows-sys` (0.52.0 via `serialport`, 0.61.2 via
  `serial2`). Warnings, not errors, and expected when two independent serial stacks are
  linked into one experiment. It would matter if both ever shipped together.
