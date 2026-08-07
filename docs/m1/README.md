# M1 acceptance hardening

M1 is a simulation-only vertical slice for one Betaflight 4.5.5 setting: the
`SYSTEM_INIT` bit in `beeper_off_flags`. It proves that the project can identify a pinned
target, take a typed backup, plan one bounded change, execute it through the central
authority gate, verify it, and classify every failure without claiming hardware support.

## Acceptance boundary

- Targets: `Mock` and project-owned `Replay` only.
- Writable scope: one four-byte `MSP_SET_BEEPER_CONFIG` payload, preserving both DShot
  fields; `MSP_EEPROM_WRITE`; and reboot.
- Hardware remains structurally refused by `WriteApproval`.
- No flashing, CLI, serial/USB adapter, UI, motor, ESC, radio or flight behavior.
- No hardware was contacted. A scope match is not a support claim.

## Hardening delivered

- Bounded version-1 `ADEJ` journal with checksummed, length-delimited records.
- Durable write-ahead evidence before every SET, SAVE and REBOOT, including recovery.
- Torn-tail recovery only; complete corruption, overwrite and overflow fail closed.
- Injected monotonic time and cancellation for Mock/Replay, with no sleeps.
- Mock/Replay parity over 26 injected failure cases.
- Separate line and branch coverage thresholds for five critical lifecycle files.
- An executable non-hardware example that prints the complete typed `M1RunReport`.

## Implemented contract map

| Contract | Owning crate | Evidence |
|---|---|---|
| Strict MSP v1 frame and typed M1 payloads | `ade-protocol-msp` | codec/parser tests and pinned records |
| Mock/Replay transport, identify guard, deadlines and cancellation | `ade-transport` | parity and control tests |
| Composite model-level identity | `ade-facts` | strict identity comparison tests |
| Pinned proposed target | `ade-capability` | scope tests; never a support claim |
| One-bit typed plan and verification requirements | `ade-planning` | changed-bit/no-op tests |
| Central command authority | `ade-execution` + `ade-safety` | exhaustive authority/recovery matrices |
| Backup-first apply and recovery | `ade-backup` + `ade-recovery` | 26 failure scenarios |
| Durable local evidence and conservative resume | `ade-casebook` | corruption/torn-tail/restart tests |
| End-to-end orchestration and report | `ade-core-api` | Mock/Replay lifecycle and example |

## Provenance guide

M1 protocol facts must resolve to the pinned records in `provenance/records/`. The exercised
set is: `mspv1-frame`, `msp-api-version`, `msp-fc-variant`, `msp-fc-version`,
`msp-board-info`, `msp-beeper-config`, `msp-set-beeper-config`, `msp-eeprom-write`,
`msp-reboot`, and `beeper-system-init-bit` (all prefixed `bf-4.5.5-`). Their source tag or
commit, retrieval date, use classification and licensing notes are validated by
`scripts/validate_provenance.py`.

`msp-uid`, status and build-info records are not exercised by this slice. In particular,
UID is not read or placed in identity, audit, backup, journal or case records. A protocol
change must update its record and pass provenance review; Mock agreement alone never raises
evidence from `MOCK_EXERCISED` to `HARDWARE_OBSERVED`.

The detailed safety boundary, failure matrix, simulation demo and acceptance contract are
in the adjacent documents. Immutable commit and Actions-run evidence belongs on the Draft
PR so it cannot be preclaimed by a document committed before CI executes.
