# ADR-0003 — Offline-First and Write Authority

- **Status:** Accepted
- **Date:** 2026-08-05

> **ملخص:** الإعداد والتشخيص يعملان بالكامل بلا إنترنت وبلا حساب. سلطة الكتابة مقيدة
> بتصنيف لكل أمر، وتصنيف استرداد معلن، وبوابات سلامة، وهرمية سلطة معرفة صارمة.

## Context

Two risks define this product. The first is privacy: a configuration tool sees hardware
identifiers, home coordinates and network names. The second is physical: a wrong write to
motors, arming or failsafe can injure someone. Neither risk is acceptable as a trade for
convenience.

## Decision

### Offline-first

- Configuration and diagnosis work **fully offline, with no account**, always.
- Network access is confined to a single component (firmware metadata, knowledge packs,
  update checks) and is always user-initiated.
- No telemetry. Local cases only. In the first product version there is no upload path at
  all; community exchange is architecturally approved but deferred.
- Case records must never contain serial numbers, USB UIDs, GPS coordinates, home position,
  network names, or any stable identifier or hash that would allow tracking one aircraft
  across sessions.

### Write authority

1. **Every command carries a classification in code** — transient MSP, persistent write, CLI
   set, save (EEPROM + reboot), bootloader reboot, DFU flash, full erase, defaults reset,
   restore. The executor **rejects** any command without a classification and without a
   declared recovery class (ADR-0005).
2. **Authority matrix:** low risk executes inside an approved plan; medium risk asks for
   contextual confirmation; anything physical is guided then executed; high risk (motors,
   flashing, firmware family change, restore) sits behind a separate explicit gate; anything
   classified `ROLLBACK_NOT_GUARANTEED` requires separate explicit approval regardless of how
   harmless it looks.
3. **Sending is not success.** Success means the value was read back, the function works, and
   nothing that previously worked stopped working.
4. **Knowledge authority hierarchy** (foundational document section 4) is enforced in code,
   not in prose: learned, community and language-model knowledge may rank hypotheses, choose
   the next diagnostic test, warn, and prefer between already-safe options. They can never
   override a safety invariant, a board capability, a firmware limit, an observed fact or an
   official rule, and can never alone move a motor, change failsafe or arming, flash, change
   firmware family, pick a value outside safe ranges, or bypass verification and recovery.
5. **Hardware writes are a separate approval gate** per milestone. Code being "ready" is not
   an argument.

## Consequences

- Some desirable features (accumulated cross-user knowledge) are deliberately slower to
  arrive.
- The executor's API is heavier: no command can be issued without classification metadata.
  This is intentional friction.
