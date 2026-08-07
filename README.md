# Autonomous Drone Expert

> **Internal working name.** The final product name is deferred (see `docs/adr/ADR-0004`).

An independent, offline-first expert for **configuring, diagnosing and programming drones**
from their components and intended flight style. Firmware compatibility engines remain
internal implementation details rather than the product identity.

## Status

**M1 acceptance candidate: simulation only, not a production release and not hardware
validated.**

M0 foundations are complete. The current M1 candidate implements one deliberately narrow
vertical slice for the Betaflight 4.5.5 `SYSTEM_INIT` beeper bit over project-owned Mock and
Replay transports: Identify → Read → Plan → Backup → Write → Save → Reboot → Reconnect →
Verify → Recovery/Report. It includes injected failure coverage, durable local journal
evidence and fail-closed resume handling.

There is still no serial/USB production transport, UI, firmware flashing, motor control or
hardware-support claim. No flight controller has been contacted, written to or flashed by
this project. See `docs/m1/README.md` for the exact acceptance boundary.

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
| Binding product contract | `docs/product/PRODUCT-CONTRACT.md` |

## Licensing

See `NOTICE.md`. There is intentionally **no `LICENSE` file**: the final license is deferred
and this repository is not licensed for distribution or use.
