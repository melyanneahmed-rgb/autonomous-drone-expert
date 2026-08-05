# ADR-0005 — Recovery Classes and State Unknown

- **Status:** Accepted
- **Date:** 2026-08-05

> **ملخص:** يُلغى مفهوم rollback العام. كل خطة وكل كتابة تعلن تصنيف استرداد قبل التنفيذ،
> والكتابة المؤقتة لها حالة تشغيلية مستقلة، وعند تعذر الإثبات تُعلن حالة سادسة صريحة.

## Context

"Rollback supported" is a comforting phrase that hides completely different realities:
rewriting a known previous value is not the same as needing a full reflash, and neither is
the same as needing the user to physically press a button. A single generic term lets the
program promise reversibility it cannot deliver — the exact failure that leaves an aircraft
in an unknown state.

## Decision

### 1. Five explicit recovery classes

| Class | Meaning |
| --- | --- |
| `AUTOMATIC_ROLLBACK_SUPPORTED` | Previous values can be rewritten and verified automatically within the same session |
| `RESTORE_FROM_BACKUP_SUPPORTED` | Recovery requires restoring a compatible backup through the plan and verification path |
| `RECOVERY_REFLASH_REQUIRED` | Recovery requires reflashing firmware and rebuilding configuration |
| `MANUAL_RECOVERY_REQUIRED` | A guided physical action is required before any software recovery |
| `ROLLBACK_NOT_GUARANTEED` | Return to the previous state cannot be guaranteed; requires separate explicit approval before execution |

Every change plan, every write and every flashing operation **declares its class before
execution**, and the class is shown to the user inside the plan. The executor rejects any
command lacking one.

A class can **degrade at runtime**: `AUTOMATIC_ROLLBACK_SUPPORTED` holds only while the
board remains reachable. If it stops responding, the situation degrades to
`MANUAL_RECOVERY_REQUIRED`, and if the outcome still cannot be proven, to state unknown.

### 2. Transient writes are not rollbacks

A value written to RAM but not committed to EEPROM is **not** "successfully rolled back"
just because it was never saved. It gets its own operational state:

```
TRANSIENT_WRITE_PENDING — RECONCILE_ON_RESUME
```

Meaning: a write was issued and not committed; the program must reconcile intent against
reality by re-reading on session resume or after reboot, before claiming either success or
failure.

### 3. A sixth readiness state

```
STATE UNKNOWN — RECOVERY REQUIRED
```

Declared whenever a write or save was issued and **neither its effect nor its reversal can
be proven**. Concretely: re-identification fails after exhausting reconnect attempts; the
value cannot be read back after reboot; a recovery was attempted without successful
verification; the case record disagrees with what the board reports; or the board returns
with a different identity.

In this state the program claims no success, declares no flight readiness, presents a guided
recovery path matching the declared class, and preserves the full audit log.

## Alternatives rejected

- **A single boolean `can_rollback`.** Rejected: collapses five materially different
  situations, and the collapse always favours optimism.
- **Silently treating an unsaved write as a no-op.** Rejected: the board may already be
  running with the transient value, which is exactly the case where honesty matters.

## Consequences

- Planning is more verbose; every item carries a class.
- Some operations become harder to offer, because declaring
  `ROLLBACK_NOT_GUARANTEED` forces an explicit approval. That friction is the point.
