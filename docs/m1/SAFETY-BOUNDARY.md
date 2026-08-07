# M1 safety boundary

## Authority

Every outbound command passes through `ade_execution::Executor`. A write requires a
`WriteApproval` whose target, write class and recovery class exactly match the operation.
`ExecutionTarget::Hardware` cannot produce that evidence.

Identification is fail closed. Only `MSP_API_VERSION`, `MSP_FC_VARIANT`,
`MSP_FC_VERSION` and `MSP_BOARD_INFO` may reach a responder or transcript during that
phase. Blocked attempts are recorded as metadata with `BlockedNotSent`.

### Command authority matrix

| Operation | Command class | Normal recovery declaration | Recovery-path declaration |
|---|---|---|---|
| Identify/snapshot read | `NoWrite` | `NotApplicableNoWrite` | same |
| `MSP_SET_BEEPER_CONFIG` | `TransientConfig` | `TransientWritePendingReconcileOnResume` | `RestoreFromBackupSupported` |
| `MSP_EEPROM_WRITE` | `PersistentConfig` | `AutomaticRollbackSupported` | `RestoreFromBackupSupported` |
| `MSP_REBOOT` | `Reboot` | `ManualRecoveryRequired` | `ManualRecoveryRequired` |

Only those five write-class/recovery-class pairs are compatible. Mock and Replay can obtain
typed approvals for them. Hardware is rejected before compatibility is considered and an
`Executor` cannot be constructed for `ExecutionTarget::Hardware`.

### Recovery decision matrix

| Durable evidence at restart | Decision |
|---|---|
| no write in flight | continue from read/reconcile path |
| transient write-ahead or applied marker | re-identify, re-read, reconcile previous/desired/third value |
| normal save or reboot in flight | reboot/read and verify; never assume persistence |
| recovery started but lacks `Restored` | `STATE UNKNOWN — RECOVERY REQUIRED` |
| terminal `Verified`, `Restored` or `StateUnknown` | rebuild only after exact evidence-chain validation |

Terminal reconstruction compares full event values, including execution target, beeper mask
and every write-ahead recovery class. Matching event names alone are not evidence.

## Durable evidence

The local journal begins with `ADEJ`, little-endian version 1 and reserved zero bytes.
Each event is encoded as:

1. little-endian `u32` payload length;
2. a bounded typed payload;
3. little-endian FNV-1a checksum of that payload.

Opening accepts only an incomplete final record as a torn append, truncates it to the last
proven boundary, and syncs before appending. Bad magic/version, an oversized record, an
invalid complete record or a checksum mismatch is corruption and is rejected. `create_new`
never overwrites an existing path. The default total bound is 64 KiB.

If a durable append, flush or sync fails, that open handle is poisoned: it refuses later
appends until the file is dropped and reopened through full validation. This prevents a
second write from guessing the end of a partially persisted record.

`WriteAhead { class, recovery }` is synced before SET, SAVE or REBOOT. A journal failure
before the first possible write aborts without sending. Once a write may have occurred,
missing evidence can only enter recovery or `STATE UNKNOWN — RECOVERY REQUIRED`.

## Explicit exclusions

There is no production transport and no claim about Windows, Android, USB, serial drivers
or real flight-controller behavior. There is no payload logging, device-derived case ID,
network upload, telemetry or background collection. `unsafe` remains forbidden.
