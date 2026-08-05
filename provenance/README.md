# Source Provenance

Every protocol fact used by this project — command identifier, payload layout, setting
name, unit, behavioural condition — must exist here as a record **before** it may appear in
code.

This is not bureaucracy. It is what keeps an independent implementation independent, and
what keeps a "fact" from silently meaning something different on a different firmware
version.

## Two independent dimensions

A record answers two questions that must never be collapsed into one:

1. **Where did this come from?** — `source_state`
2. **What has our implementation actually done with it?** — `verification_state`

Collapsing them produced a circular rule: a fact could not enter the code until it had been
exercised, and it could not be exercised until code existed to exercise it. The two
dimensions remove that deadlock without weakening any honesty guarantee.

### `source_state`

| Value | Meaning |
| --- | --- |
| `PINNED_SOURCE_RECORDED` | The fact was recorded from an official source tied to a fixed tag or commit. **No claim is made that it was ever exercised.** |

### `verification_state`

| Value | Meaning |
| --- | --- |
| `NOT_REPRODUCED` | Documented only. Our code has never exercised it. |
| `MOCK_EXERCISED` | Exercised against `ade-mock-fc`. |
| `HARDWARE_OBSERVED` | Observed on real hardware running the exact recorded firmware version. |

**What the mock does and does not prove.** `MOCK_EXERCISED` tests *our implementation and
its internal consistency*. The mock is built from the same records as the code, so it
cannot independently confirm that the official source is correct — if we misread the
source, the mock will happily agree with us. Only `HARDWARE_OBSERVED` is evidence about
the real world.

## Rules

1. **Pinned sources only.** Every record cites a **tag or commit**. Moving branches
   (`master`, `main`, `HEAD`) are rejected by CI. Upstream renames, renumbers and removes
   things between releases; a fact without a pinned version is not a fact.
2. **No copying.** Code, comments, tests, fixtures, generated tables, error strings and
   internal structure are never copied or mechanically translated. What is recorded here is
   a **restated interface fact**, in our own words.
3. **Honest state.** A state above `NOT_REPRODUCED` must carry the date that earned it.
   CI enforces this.
4. **A payload layout may be documented at any verification state**, provided it comes from
   a pinned source. Documenting a layout is not exercising it. A layout recorded at
   `NOT_REPRODUCED` is: documented from a pinned source, not yet exercised on the mock, and
   not yet observed on hardware — and it must never be described as anything more.
5. **Code follows records, not the other way round.** No protocol fact enters
   `crates/protocol-msp` or `crates/protocol-cli` without a pinned source record, an
   approved implementation pull request, and the mock tests appropriate to it in that same
   pull request. **Hardware support is never claimed below `HARDWARE_OBSERVED`.**

## Source types

| Type | Meaning |
| --- | --- |
| `official_documentation` | Published specification or official documentation, pinned by version. |
| `upstream_firmware_source_read` | Interface fact read from published firmware source and restated. No material copied. |
| `program_observed` | Observed by this program against hardware or the mock. |
| `manufacturer_documentation` | Component vendor documentation, pinned by document version. |

Community sources (forums, blogs, video) may be used **only** to discover that a problem
exists. They are never a record source, and never feed a decision until proven from one of
the types above.

## Layout

```
provenance/
  README.md                this file
  schema.json              the record schema, enforced by scripts/validate_provenance.py
  records/*.json           one record per fact
```

## Current records

Twelve Betaflight MSP command identifiers, all read from tag `4.5.5`, all
`PINNED_SOURCE_RECORDED` / `NOT_REPRODUCED`.

**Not yet recorded, and therefore not usable:** payload layouts for any command, the beeper
condition mask semantics (direction of the mask and bit positions), and the CLI entry and
exit sequences. These are required by ADR-0006 and must be documented from tag `4.5.5`
before any implementation.

## A naming collision worth knowing about

The Hardware Knowledge Database in the foundational document (section 7) has its **own,
separate** source-classification vocabulary, which also contains a value named
`UNVERIFIED`. That vocabulary classifies knowledge about physical components. It is
unrelated to the record states on this page, and the two must not be conflated.

## A correction worth keeping

Earlier planning documents referred to `MSP_SET_REBOOT`. No such name exists in the source.
The correct name is `MSP_REBOOT`. This is exactly the class of error that pinned records
prevent.
