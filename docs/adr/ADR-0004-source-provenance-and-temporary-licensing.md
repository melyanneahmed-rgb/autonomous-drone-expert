# ADR-0004 — Source Provenance and Temporary Licensing

- **Status:** Accepted. The record-state model in decision point 3 is **superseded by
  ADR-0008**; everything else stands.
- **Date:** 2026-08-05
- **Note:** engineering risk-management posture, **not legal advice**.

> **ملخص:** كل حقيقة بروتوكولية لها سجل مصدر بمرجع مثبّت. لا نسخ كود. الترخيص النهائي
> مؤجل، والمستودع Private بلا ملف LICENSE، وموانع النشر مفروضة في CI.

## Context

Betaflight, INAV and their configurators are published under GPL-3.0. The protocols
themselves — MSP, DfuSe — are published specifications. Confusing "using an open protocol"
with "copying a licensed implementation" is the single largest legal risk to an independent
project, and it is usually created by well-intentioned copy-paste during debugging.

## Decision — provenance

1. **Every protocol fact requires a record** under `provenance/records/` before it may appear
   in code: command identifiers, payload layouts, setting names, units, behavioural
   conditions.
2. **Pinned sources only.** A record must cite a tag or commit. A moving branch (`master`,
   `main`, `HEAD`) is rejected by CI. This exists because upstream renames, renumbers and
   removes things between releases, and a fact without a version is not a fact.
3. **Record states — superseded by ADR-0008.** This ADR originally defined a single status
   ladder. It is replaced by two independent dimensions: `source_state` (where the fact came
   from) and `verification_state` (`NOT_REPRODUCED` → `MOCK_EXERCISED` →
   `HARDWARE_OBSERVED`, what our code has done with it). The underlying rule is unchanged:
   nothing is described as verified because it looks correct.
4. **No copying**, ever: code, comments, tests, fixtures, generated tables, error strings,
   internal module structure. Facts are described in our own words. Golden packets are
   produced by us, not taken from upstream fixtures.
5. **Protocol-layer review:** any change under `crates/protocol-*` requires matching
   provenance records and owner review.
6. **Requires legal review before inclusion:** target definitions, official presets, any
   table derived from GPL repositories, external fixtures, any GPL/AGPL library, cryptography
   crates, MPL-2.0 files if modified, and any third-party asset.
7. **Interim strategy that avoids the question entirely:** read capabilities from the board
   at runtime instead of shipping derived tables. This is the approved approach through M7.
8. **The pipeline does not detect derivation.** CI blocks structural coupling only. This
   policy and human review are the actual control against copying (ADR-0001).

## Decision — temporary licensing posture

- Repository is **private**.
- There is **no `LICENSE` file**. Under default copyright, absence grants nothing; adding a
  license early is a promise that is hard to withdraw.
- `NOTICE.md` states: internal development work, all rights reserved temporarily, no
  permission to distribute or use, final license deferred, engineering posture not legal
  advice.
- **Technical publication blocks in CI**, not intentions: the build fails if a `LICENSE` file
  appears or a release/publication workflow is added.
- The final license is decided only after the dependency table is pinned and machine-verified,
  the provenance policy is applied in practice, the presets/target-definitions question is
  resolved, the commercial model is chosen, and competent legal review is complete — and in
  any case **before any distribution**.

## Facts recorded (as facts, not as legal conclusions)

- Betaflight and INAV firmware and configurators are published under GPL-3.0.
- `dfu-util` is published under GPL-2.0. Running it as a separate external process is an
  engineering option that reduces coupling compared with direct linking, and still requires
  legal review and documentation before distribution.
- Tauri is published under MIT/Apache-2.0.

## Consequences

- Slower protocol work: nothing lands without a record.
- The project retains full freedom of licensing choice.
