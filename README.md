# Autonomous Drone Expert

> **Internal working name.** The final product name is deferred (see `docs/adr/ADR-0004`).

An independent engineering platform for **configuring, diagnosing and programming drones**
that run Betaflight or INAV firmware.

## Status

**Foundation stage. There is no production release, no working application, and no usable
functionality in this repository yet.**

This repository currently contains only:

- the approved foundational document and M0 gate records,
- architecture decision records (ADRs),
- an empty structural Rust workspace (no logic, no dependencies),
- source-provenance records and policy,
- CI policy gates.

Nothing here talks to hardware. No flight controller has ever been connected, written to,
or flashed by this project.

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

## Licensing

See `NOTICE.md`. There is intentionally **no `LICENSE` file**: the final license is deferred
and this repository is not licensed for distribution or use.
