# Source Provenance

Every protocol fact used by this project — command identifier, payload layout, setting name,
unit, behavioural condition — must exist here as a record **before** it may appear in code.

This is not bureaucracy. It is what keeps an independent implementation independent, and what
keeps a "fact" from silently meaning something different on a different firmware version.

## Rules

1. **Pinned sources only.** Every record cites a **tag or commit**. Moving branches
   (`master`, `main`, `HEAD`) are rejected by CI. Upstream renames, renumbers and removes
   things between releases; a fact without a pinned version is not a fact.
2. **No copying.** Code, comments, tests, fixtures, generated tables, error strings and
   internal structure are never copied or mechanically translated. What is recorded here is a
   **restated interface fact**, in our own words.
3. **Honest status.** A record starts `UNVERIFIED`. It becomes `MOCK_VERIFIED` only when
   reproduced against `ade-mock-fc`, and `HARDWARE_VERIFIED` only when observed on real
   hardware running the exact recorded firmware version.
4. **Never mark a payload layout verified because it looks right.** Verified means observed.
5. **Code follows records, not the other way round.** No constant enters
   `crates/protocol-msp` or `crates/protocol-cli` without a matching record.

## Status ladder

| Status | Meaning |
| --- | --- |
| `UNVERIFIED` | Read from a pinned published source. Not yet reproduced anywhere. |
| `MOCK_VERIFIED` | Reproduced against the mock flight controller. |
| `HARDWARE_VERIFIED` | Observed on real hardware running the exact recorded firmware version. |

## Source types

| Type | Meaning |
| --- | --- |
| `official_documentation` | Published specification or official documentation, pinned by version. |
| `upstream_firmware_source_read` | Interface fact read from published firmware source and restated. No material copied. |
| `program_observed` | Observed by this program against hardware or the mock. |
| `manufacturer_documentation` | Component vendor documentation, pinned by document version. |

Community sources (forums, blogs, video) may be used **only** to discover that a problem
exists. They are never a record source, and never feed a decision until proven from one of the
types above.

## Layout

```
provenance/
  README.md                this file
  schema.json              the record schema, enforced by scripts/validate_provenance.py
  records/*.json           one record per fact
```

## Current records

Twelve Betaflight MSP command identifiers, all read from tag `4.5.5`, all `UNVERIFIED`.

**Not yet recorded, and therefore not usable:** payload layouts for any command, the beeper
condition mask semantics (direction of the mask and bit positions), and the CLI entry and exit
sequences. These are required by ADR-0006 and must be established from tag `4.5.5` before any
implementation.

## A correction worth keeping

Earlier planning documents referred to `MSP_SET_REBOOT`. No such name exists in the source.
The correct name is `MSP_REBOOT`. This is exactly the class of error that pinned records
prevent.
