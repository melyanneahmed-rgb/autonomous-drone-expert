# M1A — Windows Serial Transport Spike

**Experiment. Not production code. Never merge into `main`.**

An isolated comparison of Rust serial-port layers for the production transport crate,
Windows first. This package declares its own `[workspace]`, so its dependencies are never
resolved by the production build and never reach a production crate.

| Document | Contents |
| --- | --- |
| [`docs/REPORT.md`](docs/REPORT.md) | The comparison, the evidence and the recommendation |
| [`docs/DEPENDENCY-AUDIT.md`](docs/DEPENDENCY-AUDIT.md) | Per-candidate audit with verified versions, licences and decisions |
| [`docs/TRANSPORT-CONTRACT.md`](docs/TRANSPORT-CONTRACT.md) | The proposed trait and why each decision was made |
| [`docs/MANUAL-HARDWARE-TEST-PLAN.md`](docs/MANUAL-HARDWARE-TEST-PLAN.md) | The 14 tests that require real Windows hardware |

## Evidence labels

Every assertion states what it actually proves:

- `CI_VERIFIED` — executed on the CI runner, real behaviour.
- `SIMULATED_ONLY` — logic exercised with synthetic input.
- `REQUIRES_WINDOWS_HARDWARE_TEST` — unreachable without a real device.

## Running it

```
cd spikes/windows-serial-transport
cargo test -- --nocapture
```

Nothing in this package opens a real device, sends MSP, or writes configuration.
