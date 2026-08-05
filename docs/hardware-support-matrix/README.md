# Hardware Support Matrix

This matrix is the **only** place where hardware support may be claimed. If a board is not
listed here with a validated status, the product does not support it — regardless of what any
other document, plan or diagnostic output might suggest.

## Status vocabulary

| Status | Meaning |
| --- | --- |
| `PROPOSED — NOT PURCHASED OR HARDWARE VALIDATED` | A candidate on paper. Never connected. No support implied. |
| `BENCH ACQUIRED — NOT VALIDATED` | The unit exists on the bench. Nothing has been proven. |
| `READ VALIDATED` | Identification and read-only inspection proven on the real unit. |
| `WRITE VALIDATED` | A write, save, reboot, reconnect and verification cycle proven on the real unit, behind the approved hardware-write gate. |
| `SUPPORTED` | Write validated, plus recovery and injected-failure scenarios proven on the real unit. |

## Current entries

| Role | Board | Betaflight target | Firmware | Status |
| --- | --- | --- | --- | --- |
| Primary candidate | SpeedyBee F405 V4 | `SPEEDYBEEF405V4` | Betaflight 4.5.5 | `PROPOSED — NOT PURCHASED OR HARDWARE VALIDATED` |
| Backup candidate | Matek F405-TE | `MATEKF405TE` | Betaflight 4.5.5 | `PROPOSED — NOT PURCHASED OR HARDWARE VALIDATED` |

**No board has been purchased. No board has been connected. No command has ever been sent to
a flight controller by this project.**

## Why these two

- STM32F405 with standard USB VCP.
- Mature, documented targets.
- BZ+/BZ- pads for a 5 V buzzer, which makes the first vertical slice testable with no
  motors, no propellers and no battery.
- Two different manufacturers, so a defect in one unit does not block the milestone.
- Intended as dedicated bench units, never taken from a flying aircraft.

## Open hardware questions

1. **Is the 5 V rail (and therefore the buzzer path) powered from USB alone on the specific
   unit?** Unproven. Because of this, audible verification is optional and never gates
   acceptance of M1 (ADR-0006).
2. **DFU entry and recovery behaviour.** The ROM bootloader path is an
   *expected recovery path requiring hardware validation*, not a guarantee.
3. **Reconnect behaviour on Windows after reboot** (port naming, timing). Unproven.

## Rule

An entry is promoted only by evidence produced by this project on that exact board and that
exact firmware version. Promotion is a pull request that cites the evidence.
