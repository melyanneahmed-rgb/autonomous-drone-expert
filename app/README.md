# `app/` — optional native product shells (not implemented)

This directory is a **placeholder**. No application exists here yet.

## Current platform decision

| Concern | Decision | Status |
| --- | --- | --- |
| Primary shell | Web/PWA | Approved (ADR-0010) |
| Native shell | Tauri 2 or another audited wrapper | Optional; capability-driven |
| Core | Rust native + WebAssembly | Approved |
| UI | TypeScript + React | Approved (ADR-0002) |
| Direction | RTL-first (Arabic), English second | Approved |
| Platform order | Web/PWA, then Windows and Android packages | Approved |

No native shell dependency is approved by this document. Any wrapper is selected, audited
and pinned only when a browser capability gap requires it.

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
