# ADR-0008 — Two-Dimensional Provenance State

- **Status:** Accepted
- **Date:** 2026-08-05
- **Supersedes:** the record-state model in ADR-0004 (decision point 3) and the code-entry
  rule in ADR-0006. Everything else in those ADRs stands.

> **ملخص:** فصل بُعدَي السجل: من أين جاءت الحقيقة (`source_state`)، وماذا فعل تنفيذنا بها
> (`verification_state`). السلّم الواحد السابق كان دائريًا: لا تدخل الحقيقة الكود قبل
> تجريبها، ولا يمكن تجريبها قبل وجود كود يجرّبها. الفصل يزيل الدائرة بلا أي تنازل عن الصدق.

## Context

ADR-0004 defined one status ladder: `UNVERIFIED` → `MOCK_VERIFIED` → `HARDWARE_VERIFIED`,
and ADR-0006 required that a fact reach at least `MOCK_VERIFIED` before entering Rust code.

That is circular. The mock is built from the records. To exercise a fact on the mock, the
mock must implement it. To implement it, the fact must be in code. To put it in code, it
must already have been exercised. Taken literally, the first protocol fact could never be
implemented at all.

The ladder also conflated two genuinely different claims:

- *"This came from Betaflight tag 4.5.5, line-referenced and restated."*
- *"Our code has run against this and behaved as expected."*

Neither implies the other. A perfectly documented fact that has never been executed is a
normal, honest state — the old vocabulary had no way to say so without the word
"unverified", which reads like a defect rather than a stage.

A third problem followed from the same conflation: the validator refused to store a payload
layout unless the record was hardware verified. So a layout published in the official source
could not be written down until hardware existed — which meant it would live in someone's
head or in a pull request comment, exactly where an unrecorded fact does the most damage.

## Decision

### Two independent fields

**`source_state`** — where the fact came from.

| Value | Meaning |
| --- | --- |
| `PINNED_SOURCE_RECORDED` | Recorded from an official source tied to a fixed tag or commit. No claim that it was ever exercised. |

**`verification_state`** — what our implementation has done with it.

| Value | Meaning |
| --- | --- |
| `NOT_REPRODUCED` | Documented only. Our code has never exercised it. |
| `MOCK_EXERCISED` | Exercised against `ade-mock-fc`. |
| `HARDWARE_OBSERVED` | Observed on real hardware running the exact recorded firmware version. |

A state above `NOT_REPRODUCED` must carry the date that earned it. CI enforces this.

### What the mock proves

`MOCK_EXERCISED` demonstrates that **our implementation is internally consistent** — the
codec, the session layer and the mock agree. Because the mock is generated from the same
records as the production code, it **cannot** independently confirm that the official
source was read correctly. A shared misreading produces a green test.

The mock is therefore never presented as evidence about Betaflight or INAV. Only
`HARDWARE_OBSERVED` is evidence about the real world.

### Payload layouts

A payload layout **may be documented at any verification state**, provided it comes from a
pinned source. The coupling to hardware verification is removed.

A layout recorded at `NOT_REPRODUCED` is exactly three things, and must never be described
as more:

- documented from a pinned source,
- not yet exercised on the mock,
- not yet observed on hardware.

### Facts entering code

The circular threshold is replaced by four rules:

1. No protocol fact enters the code without a **pinned source record**.
2. Bringing it in requires an **approved implementation pull request**.
3. That **same pull request** adds the mock tests appropriate to the fact.
4. **Hardware support is never claimed below `HARDWARE_OBSERVED`.**

Rule 3 is what the old threshold was actually reaching for, expressed in a way that can be
satisfied: the tests arrive *with* the implementation rather than before it.

## Alternatives rejected

- **Keep the ladder, grant exceptions for bootstrapping.** Rejected: a rule with a standing
  exception is not a rule, and the exception would be invoked precisely when discipline
  matters most.
- **Allow code first, record afterwards.** Rejected: undermines the entire provenance
  control, which is the only real defence against derivation (ADR-0001, ADR-0004).
- **Treat the mock as verification of the source.** Rejected: it is verification of *us*.
  Saying otherwise would manufacture false confidence, which is the failure mode this
  project exists to avoid.

## Consequences

- Records become slightly more verbose: two fields instead of one.
- Payload layouts can be documented early, which is where they belong.
- The mock can finally be built without violating the rules.
- Every claim of hardware support still requires hardware. Nothing was weakened.
