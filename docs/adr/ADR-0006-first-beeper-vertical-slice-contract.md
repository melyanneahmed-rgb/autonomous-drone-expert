# ADR-0006 — First Beeper Vertical Slice Contract

- **Status:** Accepted — **documentation only**. Nothing in this ADR is implemented, and
  implementation is **not approved** in the foundation batch.
- **Date:** 2026-08-05

> **ملخص:** عقد شريحة M1 الرأسية الأولى: إعداد الجرس والتحقق منه على لوحة وإصدار محددين
> بالضبط، عبر الدورة الكاملة بما فيها فشل محقون واسترداد وتقرير وتسجيل حالة. توثيق فقط.

## Purpose

The first slice is deliberately trivial in effect and complete in shape: it must exercise
the entire lifecycle end to end so that the execution core is proven before anything
risky is attempted. A beeper condition changes no flight behaviour, needs no battery, no
motors and no propellers.

## Target and firmware

| Item | Value | Status |
| --- | --- | --- |
| Board | SpeedyBee F405 V4 | `PROPOSED — NOT HARDWARE VALIDATED` |
| Betaflight target | `SPEEDYBEEF405V4` | `PROPOSED — NOT HARDWARE VALIDATED` |
| Firmware | Betaflight 4.5.5 | Pinned for the slice |
| MSP API version | 1.46 | Recorded from tag `4.5.5`, status `UNVERIFIED` |

The board is proposed, not purchased, not connected and not validated. Nothing in this
document may be read as a support claim.

## Contract (future work — not implemented)

1. Read `MSP_BEEPER_CONFIG`.
2. Store the **full expected 9-byte payload** — only after its layout is proven under the
   provenance policy. Until then the layout is `UNVERIFIED` and must not be assumed.
3. Change **`beeper_off_flags` only**. No other field is touched.
4. Change **only the `SYSTEM_INIT` bit**, and only after its meaning and bit position are
   established from tag `4.5.5`. The direction of the mask (enable versus disable) is itself
   an unproven fact.
5. Send a **4-byte payload** in `MSP_SET_BEEPER_CONFIG`, so that the DShot beacon fields are
   left untouched rather than rewritten with assumed values.
6. Re-read before saving.
7. Perform the EEPROM write **only after the Hardware Write gate is separately approved**.
8. Perform a normal reboot.
9. Reconnect and re-read the full 9 bytes.
10. Verify that the mask changed exactly as intended **and that the DShot fields did not
    change**.
11. **Electronic verification is the primary success criterion.**
12. **Audible verification is optional** until it is proven on the actual unit that BZ+/BZ-
    and the buzzer operate on USB power alone. It never gates acceptance.
13. **No CLI** in the first beeper slice.
14. **No `diff all`** is required in the first slice.
15. The first backup is deliberately limited: board and firmware identity, the values read,
    the frame log, and the checkpoints.
16. The transient write before EEPROM commit uses
    `TRANSIENT_WRITE_PENDING — RECONCILE_ON_RESUME` (ADR-0005), never "rolled back".
17. Before any EEPROM write, the program must establish an **exclusive session**: no other
    MSP client and no overlapping Bluetooth or wireless link to the same board.

## Recovery classes for the slice

| Step | Class |
| --- | --- |
| Identify / read / snapshot | No write — not applicable |
| Transient `MSP_SET_BEEPER_CONFIG` | `TRANSIENT_WRITE_PENDING — RECONCILE_ON_RESUME` |
| `MSP_EEPROM_WRITE` | `AUTOMATIC_ROLLBACK_SUPPORTED` while the board stays reachable; degrades to `RESTORE_FROM_BACKUP_SUPPORTED` if local state is lost |
| `MSP_REBOOT` | No configuration change; if reconnection fails, `MANUAL_RECOVERY_REQUIRED` |
| Failed or unprovable recovery | `STATE UNKNOWN — RECOVERY REQUIRED` |

## Mandatory injected-failure scenarios (mock and replay)

Mask mismatch after reboot; EEPROM write timeout or no reply; corrupt frame or bad checksum;
duplicated and out-of-order replies; device never returns after reboot; device returns with a
**different identity**; process kill between write and save, between save and reboot, and
during case recording; serial disconnect during each of the four phases; board power loss
during EEPROM write; port busy, permission denied and missing driver at connect.

## Identification criteria

No fixed time threshold. Median and p95 are measured and documented after implementation.
Timeouts are configurable. The interface must stay responsive and cancellable. Port busy,
permission denied and missing driver each produce a **named** diagnosis. **Zero write
commands during identification**, asserted by test. Every outbound frame is audited. Ports
are never opened at random without user selection or a trusted identifier match.

## Status of every command referenced here

All command identifiers are recorded under `provenance/records/` with status `UNVERIFIED`.
None may be placed in Rust code until its record reaches at least `MOCK_VERIFIED` and the
implementation batch is approved.
