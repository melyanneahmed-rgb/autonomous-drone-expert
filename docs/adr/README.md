# Architecture Decision Records

Each ADR records one decision: its context, the decision itself, the alternatives that were
rejected and why, and the consequences.

| ADR | Title | Status |
| --- | --- | --- |
| [ADR-0001](ADR-0001-independent-repository-and-workspace-isolation.md) | Independent Repository and Workspace Isolation | Accepted |
| [ADR-0002](ADR-0002-tauri-rust-react-architecture.md) | Tauri 2 + Rust + React Architecture | Superseded for product shell order by ADR-0010 |
| [ADR-0003](ADR-0003-offline-first-and-write-authority.md) | Offline-First and Write Authority | Accepted |
| [ADR-0004](ADR-0004-source-provenance-and-temporary-licensing.md) | Source Provenance and Temporary Licensing | Accepted |
| [ADR-0005](ADR-0005-recovery-classes-and-state-unknown.md) | Recovery Classes and State Unknown | Accepted |
| [ADR-0006](ADR-0006-first-beeper-vertical-slice-contract.md) | First Beeper Vertical Slice Contract | Accepted (documentation only) |
| [ADR-0007](ADR-0007-firmware-capability-pack-architecture.md) | Firmware Capability Pack Architecture | Accepted |
| [ADR-0008](ADR-0008-two-dimensional-provenance-state.md) | Two-Dimensional Provenance State | Accepted — supersedes part of ADR-0004 and ADR-0006 |
| [ADR-0009](ADR-0009-first-party-workspace-path-dependencies.md) | First-Party Workspace Path Dependencies | Accepted |
| [ADR-0010](ADR-0010-web-pwa-product-shell-and-responsibility-boundary.md) | Web/PWA Product Shell and Responsibility Boundary | Accepted — supersedes part of ADR-0002 |
| [ADR-0011](ADR-0011-audited-web-dependency-and-repository-policy.md) | Audited Web Dependency and Repository Policy | Accepted — supersedes ADR-0002 package manager/layout |

Rules: an ADR is never edited to change its decision. It is superseded by a new ADR that
references it.
