# ADR-0010 — Web/PWA Product Shell and Responsibility Boundary

- **Status:** Accepted
- **Date:** 2026-08-07
- **Supersedes:** ADR-0002 for primary shell/platform order; foundational document sections
  1, 3, 11 and 13 where they conflict with this decision.

## Context

The accepted M1 Rust safety core is sound, but the old platform plan described a
Windows-first Tauri application and automatic AUX mapping. The clarified product identity is
an independent, offline-first drone expert: the user declares non-discoverable components,
chooses a flight intent, and the application configures every other supported setting. The
only manual configuration decision is assigning physical transmitter switches/buttons to
functions.

The product must also offer a trusted online firmware download and a manual local file path,
while keeping flashing behind its own approval and recovery gate. Internal firmware-family
adapters must not become the product's visible identity.

## Decision

1. **Product surface:** TypeScript/React Web/PWA is the first product shell. It is RTL-first,
   installable, and works offline after installation. Packaged Windows and Android builds
   reuse the product interface and deterministic core; native shells exist for capabilities
   the browser cannot provide reliably.
2. **Core:** Rust remains the safety-critical deterministic core and is separated from I/O
   so it can target WebAssembly and native platforms. Browser transport and storage are
   asynchronous adapters (Web Serial and IndexedDB); native adapters remain replaceable.
3. **No protocol in the UI:** the UI consumes product contracts only. It never builds MSP or
   CLI frames, never selects raw technical values, and never owns write authority.
4. **Independent identity:** firmware-family names and configurator brands remain internal
   compatibility/provenance facts. They are not shown in the ordinary product interface.
5. **Responsibility boundary:** every supported setting domain is derived, validated,
   applied and verified automatically except `ControlFunctionAssignments`. The application
   observes live inputs and validates conflicts, but the user explicitly chooses which
   switch/button performs each function.
6. **Firmware acquisition:** the user may request a trusted online download or select a local
   firmware file. Both paths require compatibility and integrity verification. Download or
   file selection never starts flashing automatically; backup, power checks, a declared
   recovery path and separate approval remain mandatory.
7. **Honest capability boundary:** Web Serial selection or a successful Mock/Replay run is
   not hardware support evidence. Hardware claims remain blocked until supervised observation
   on the exact board and firmware.

## Consequences

- ADR-0002 remains historical evidence for the earlier Tauri-first decision; this ADR is the
  current decision where the two conflict.
- M1 behaviour and its safety invariants do not change.
- The next implementation work is general product-domain contracts, I/O separation,
  offline PWA support, then read-only Web Serial identification.
- DFU, drivers, and platform limitations may still require native Windows/Android adapters;
  those adapters do not fork product logic or weaken the executor.
