# ADR-0012 — Audited storage-only WASM binding exception

- **Status:** Accepted — dependency and policy decision only
- **Date:** 2026-08-09
- **Narrows:** ADR-0009. Historical ADR-0009 remains unchanged.

## Context

ADR-0009 permits only first-party workspace path dependencies. That posture protected the
initial core, but it also prevents a safe, stable browser-callable WebAssembly boundary.
The compiler proof recorded for the storage bridge found no maintainable dependency-free
route that simultaneously provides named browser exports, typed value marshalling and host
callbacks while every first-party crate retains `#![forbid(unsafe_code)]`. Raw pointer ABIs,
unsafe export attributes, linker tricks and mangled-symbol discovery are not acceptable
substitutes.

The owner completed separate exact-version audits for the product library and proposed
tooling. This ADR records that reviewed exception without converting the repository into a
general external-dependency workspace.

## Decision

### Product dependency

Exactly one external Rust product dependency is authorised, and only in the dedicated
storage bridge crate:

```toml
wasm-bindgen = { version = "=0.2.127", default-features = false, features = ["std"] }
```

The product MSRV remains Rust 1.85.0. The audited product closure compiles for
`wasm32-unknown-unknown` on that toolchain. `js-sys`, `web-sys`,
`wasm-bindgen-futures`, `serde`, `serde-wasm-bindgen`, `wasm-pack` and new npm dependencies
are not authorised.

### Isolated build tooling

The complete `wasm-bindgen-cli 0.2.127` is rejected. Its locked `rouille` path contains
unmaintained crates covered by RUSTSEC-2023-0028, RUSTSEC-2023-0050,
RUSTSEC-2023-0081 and RUSTSEC-2021-0146. No advisory ignore or suppression is authorised.

Instead, one repository-owned tool outside the product workspace may depend on:

```toml
wasm-bindgen-cli-support = { version = "=0.2.127" }
```

The tool has its own manifest, lock and cargo-deny configuration. It may declare a tooling
MSRV of Rust 1.86 or newer and must not raise the product workspace MSRV. It is build-time
only and is never linked into, packaged with or exposed by the product runtime.

The tooling closure contains `foldhash 0.2.0` under the Zlib license through
`hashbrown 0.16.1` and `wasmparser 0.245.1`. Zlib is approved only for that exact package in
that isolated tooling graph. It is not a workspace-wide product license allowance. Duplicate
`hashbrown 0.16.1/0.17.1` and `syn 2.0.119/3.0.3` remain visible warnings under the existing
`multiple-versions = "warn"` policy.

### Unsafe distinction

First-party Rust remains subject to `#![forbid(unsafe_code)]` and
`scripts/check_forbid_unsafe.py`. No first-party unsafe is approved. The audit did observe
and accept third-party unsafe inside `wasm-bindgen` and its reviewed closure. Accepting that
external implementation does not weaken the repository-owned source rule.

### Machine enforcement

`scripts/check_isolation.py` admits only two complete declaration shapes:

1. `wasm-bindgen` in `[dependencies]` of
   `crates/web-storage-wasm-bridge/Cargo.toml` with the exact version and features above.
2. `wasm-bindgen-cli-support` in `[dependencies]` of
   `tools/wasm-bindgen-cli-support/Cargo.toml` at exact version 0.2.127.

Every other registry dependency remains denied. The gate also rejects version or feature
drift, table drift, aliases, wildcard/git/registry overrides, placement in unrelated crates,
full `wasm-bindgen-cli`, tooling leakage into the product workspace and silent growth of the
exception map. Existing first-party path checks are unchanged.

## Authority boundary

This decision authorises only the storage effect lane required to connect the real Rust
`EffectJournalStore` to the existing browser IndexedDB byte/CAS host. It grants no Web
Serial, USB, MSP transport, flight-controller command, WriteApproval, DFU, motor, flashing,
firmware, restore, telemetry, cloud/network, APK or UI-redesign authority. A generic
`IoEffect` boundary must not expose `TransportEffect` to JavaScript.

The browser host remains non-authoritative byte storage. Rust owns ADEJ parsing, repair,
journal semantics, revisions, response correlation, resume decisions and recovery state.
Request IDs and storage revisions cross the boundary as validated canonical u64 decimal
text or BigInt, never JavaScript `Number`.

## Consequences

- The root Cargo lock must record the exact audited product graph and pass all four required
  cargo-deny checks.
- The tooling lock must be committed separately and pass the same checks under its narrow
  Zlib exception.
- The bridge and tooling remain first-party safe Rust.
- A real Chrome test must prove Rust WASM execution against the actual IndexedDB adapter,
  including empty load, Rust-owned torn-tail repair, stale CAS conflict, response
  correlation and values above `2^53`.
- This ADR does not approve another dependency, another version, or another product surface.
