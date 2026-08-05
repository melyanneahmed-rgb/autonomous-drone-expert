# ADR-0002 — Tauri 2 + Rust + React Architecture

- **Status:** Accepted. Tauri remains **provisional** and is re-evaluated at M8 (DFU and
  Windows driver management).
- **Date:** 2026-08-05

> **ملخص:** Tauri 2 مع نواة Rust وواجهة TypeScript/React، ونظام Windows أولًا. الإصدارات
> تُثبَّت بدقة بعد تدقيقها، والـ MSRV مفهوم منفصل عن سلسلة الأدوات المثبتة.

## Context

The product must reach USB serial ports, enumerate USB devices, and eventually drive DFU;
it must reconnect reliably after a board reboots; it must ship signed updates; it must work
fully offline; it must be testable; and it must render Arabic RTL correctly. Windows is the
primary platform, because that is where the audience is and where device drivers are
hardest.

## Decision

**Shell: Tauri 2.** Chosen for a small attack surface (the web layer reaches the system only
through an explicitly declared command surface), small binaries, permissive licensing, and a
full-capability Rust core. Electron was rejected: heavier, larger, and a wider attack
surface with no compensating advantage for this workload. A pure web application was
rejected: DFU driver handling on Windows, absent Safari support and browser permission
friction make "plug it in and it works" fragile for beginners. A local service plus web UI
was rejected as operationally confusing for the target audience.

**Core: Rust.** Memory safety in the layer that parses input from an untrusted USB device,
strong test tooling, and a future path to WebAssembly if a web edition is ever pursued.

**UI: TypeScript (strict) + React.** Chosen not for popularity but because the interface is
a complex state machine — dynamic questions, plans, guided tests, an always-available
emergency stop — where mature accessibility, internationalisation and testing libraries
matter more than raw framework minimalism. Svelte is lighter and cleaner but weaker across
exactly that band. The cost is accepted because no performance-critical work lives in the UI.

**Package manager: pnpm.** Strict isolation and reliable lockfiles.

**Repository layout: monorepo** — one Cargo workspace and one pnpm workspace, so a change
crossing core and UI is a single reviewable commit.

**Third-party serial/USB plugins are rejected on principle.** The protocol and transport
layers are ours, behind our own interfaces (ADR-0003, foundational document section 11).

## Versions and toolchain

- **No "latest" claims.** The Tauri version is whatever stable, reviewed release exists when
  the application batch is created; it is audited and then pinned exactly in lockfiles.
- **Pinned toolchain:** `rust-toolchain.toml` pins an exact Rust version, verified against
  the official release channel at pin time (`1.97.1`, released 2026-07-14, verified
  2026-08-05).
- **MSRV:** `rust-version` in `Cargo.toml` (`1.85.0`, the minimum that supports edition
  2024). The pinned toolchain and the MSRV are **different things** and must never be
  conflated. Raising either is a dedicated pull request.
- **Edition 2024.**
- **Nightly is never the project toolchain.** A future fuzzing job pins its own nightly.

## Keeping macOS and Linux viable while shipping Windows first

1. OS-specific code is confined to `crates/transport`; CI rejects platform conditionals
   elsewhere once that check is added.
2. Linux and Windows both run in CI from the first commit, because Linux is cheap and
   catches drift early.
3. macOS enters CI at M5 as a build target and becomes fully supported at M11.

## Consequences

- The DFU and Windows driver story (M8) is the real test of this decision; the re-evaluation
  point is deliberately placed there.
- The UI can never be a shortcut to the hardware: it talks only to `ade-core-api`.
