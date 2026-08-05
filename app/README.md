# `app/` — desktop application shell (not implemented)

This directory is a **placeholder**. No application exists here yet.

## Provisionally approved platform

| Concern | Decision | Status |
| --- | --- | --- |
| Shell | Tauri 2 | Provisionally approved (ADR-0002) |
| Core | Rust | Approved |
| UI | TypeScript + React | Approved (ADR-0002) |
| Direction | RTL-first (Arabic), English second | Approved |
| Primary platform | Windows-first, then macOS and Linux | Approved |

The exact Tauri version is **not** pinned here. It will be the latest stable, reviewed
release at the time the application batch is created, audited and then pinned exactly in
the lockfiles.

## Not in this batch

- No Tauri project, configuration, capabilities or permissions files.
- No Tauri runtime dependency of any kind.
- No build or bundling configuration.

## Architectural rule that this directory must always honour

The shell exposes a **narrow, explicitly defined command surface** over `ade-core-api`.
The user interface never sees MSP, never sees the CLI channel, and never issues a
protocol frame. Any action reaching hardware must first be produced as a structured
action by the deterministic engines, then pass the safety validator and the plan
executor.
