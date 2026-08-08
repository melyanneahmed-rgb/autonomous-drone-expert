# Autonomous Drone Expert

> **Internal working name.** The final product name is deferred (see `docs/adr/ADR-0004`).

An independent, offline-first expert for **configuring, diagnosing and programming drones**
from their components and intended flight style. Firmware compatibility engines remain
internal implementation details rather than the product identity.

## Status

**M1 accepted on simulation; G1 merged; M2 product-core work in progress. This is not a
production release and is not hardware validated.**

M0 foundations are complete. The current M1 candidate implements one deliberately narrow
vertical slice for the Betaflight 4.5.5 `SYSTEM_INIT` beeper bit over project-owned Mock and
Replay transports: Identify → Read → Plan → Backup → Write → Save → Reboot → Reconnect →
Verify → Recovery/Report. It includes injected failure coverage, durable local journal
evidence and fail-closed resume handling.

M2 now builds the protocol-independent product plan and an effect boundary between the
deterministic Rust core and asynchronous Web/native transport and storage adapters. The new
boundary performs no I/O and adds no hardware authority; CI cross-compiles its first crates
for `wasm32-unknown-unknown`. See `docs/m2/README.md`.

The deployed PWA shell is not yet repository-integrated with this Rust core. There is still
no production serial/USB transport, firmware flashing, motor control or hardware-support
claim. No flight controller has been contacted, written to or flashed by this repository.
See `docs/m1/README.md` for the exact acceptance boundary.

## What this project is

A platform built on three intelligence pillars over a deterministic execution core:

1. **Hardware Knowledge Database** — what a component can do, its limits and known issues.
2. **Intelligent Diagnostic Expert** — investigates faults like an engineer, not an error message.
3. **Knowledge Engine** — accumulates verified experience under strict governance.

The execution core follows one lifecycle:

```
Discover -> Decide -> Plan -> Apply -> Reboot -> Verify -> Diagnose -> Recovery
```

## What this project is not

- Not a fork, clone or reskin of Betaflight Configurator or INAV Configurator.
- Not derived from any previous project of the author.
- Not a read-only inspector: the end product programs the aircraft and verifies the result.

## Independence

This is a standalone repository with its own codebase, architecture, tests and release
lifecycle. It does not depend on, import from, or share code with any other repository.

## Authoritative documents

| Document | Path |
| --- | --- |
| Foundational document v1.1 (single source of truth) | `docs/foundational/v1.1.md` |
| M0 Foundations Gate | `docs/m0/foundations-gate.md` |
| M0 Amendment | `docs/m0/amendment.md` |
| Final execution review | `docs/m0/final-execution-review.md` |
| Architecture decisions | `docs/adr/` |
| Source provenance policy | `provenance/README.md` |
| Hardware support matrix | `docs/hardware-support-matrix/README.md` |
| M1 simulation acceptance candidate | `docs/m1/README.md` |
| M2 Product Core boundary | `docs/m2/README.md` |
| Web dependency audit and policy | `docs/m2/WEB-DEPENDENCY-AUDIT.md` |
| Approved Site v3 provenance boundary | `docs/m2/SMART-CONFIGURATOR-SITE-V3-PROVENANCE.md` |
| Binding product contract | `docs/product/PRODUCT-CONTRACT.md` |

## Licensing

See `NOTICE.md`. There is intentionally **no `LICENSE` file**: the final license is deferred
and this repository is not licensed for distribution or use.
