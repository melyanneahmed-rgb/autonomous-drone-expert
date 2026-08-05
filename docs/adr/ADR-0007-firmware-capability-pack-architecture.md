# ADR-0007 — Firmware Capability Pack Architecture

- **Status:** Accepted
- **Date:** 2026-08-05
- **Scope:** architecture principles only. **No production capability pack is created in the
  foundation batch.**

> **ملخص:** التطبيق الأساسي يحمل قدرات التعرف الأساسية فقط. معرفة كل firmware تعيش في حزم
> قدرات **وصفية بلا كود**، موقعة وversioned وقابلة للإبطال، لا تستطيع تنفيذ كود ولا لمس
> العتاد ولا تجاوز مدقق السلامة. غياب حزمة موثوقة يعني قراءة آمنة فقط أو منع الكتابة.

## Context

Betaflight and INAV diverge, and each diverges from itself across versions: commands are
added, renumbered, given different payloads, and removed. Betaflight has additionally moved
from `4.x` numbering to a year-based scheme, and cloud builds mean a given board may not even
contain a feature its version nominally supports.

Encoding every one of those differences inside the application binary produces a program that
must be rebuilt and re-released to support one new firmware release, and whose safety-critical
core grows unboundedly with knowledge that is not safety-critical. That does not scale, and it
puts volatile knowledge inside the most stable layer.

## Decision

### 1. The base application knows almost nothing about firmware

It carries only the **minimum identification capability** required to read:

- firmware family,
- API version,
- firmware version,
- board target and identifier.

That is enough to answer "what am I talking to?" — and nothing more.

### 2. Four separate artefact types, never conflated

| Artefact | What it is | What it is not |
| --- | --- | --- |
| **Firmware binary** | The image flashed onto the board | Not readable knowledge; the program never infers behaviour from it |
| **Firmware Capability Pack** | Descriptive knowledge about one firmware family and version range | Not code, not a driver |
| **Hardware Knowledge Pack** | Knowledge about physical components | Not firmware knowledge |
| **Rules / Profile Pack** | Goal-to-configuration rules and profile values | Not protocol knowledge |

### 3. Capability packs are purely descriptive

A pack is **data**. It contains **no executable code** of any kind — no scripts, no
expressions evaluated as code, no bytecode.

### 4. Contents (as the packs mature)

Firmware family; supported version and API ranges; command schemas; payload layouts; settings
and capability maps; save and reboot requirements; verification rules; recovery
classifications; compatibility limits.

### 5. Distribution

A pack may be used locally, downloaded from a trusted source, or imported manually by the
user. Manual import must always remain possible so that the program stays usable offline and
in restricted environments.

### 6. Integrity requirements

Every pack is **signed**, carries a **checksum**, is **versioned**, is **revocable**, and
declares a **minimum compatible application version**. Consistent with the Knowledge Engine
governance in the foundational document, a pack also carries its schema version, review or
expiry date, revocation identifier, rollback version and signing key identifier. An expired,
revoked or incompatible pack is not used for any decision, and the system falls back to the
declared rollback version.

### 7. Hard prohibitions

A pack may **never**:

- execute code,
- talk to hardware directly,
- bypass the Safety Validator,
- send MSP or CLI commands directly,
- change a recovery class,
- select a value outside the safe ranges.

A pack **describes**. The engines **decide**, the safety validator **permits**, the executor
**acts**.

### 8. Behaviour when no trusted pack exists

The program degrades honestly to one of:

- **Minimal identification and read-only operation** where that is safe, or
- **`UNSUPPORTED FIRMWARE FOR WRITES`**.

It never guesses its way into writing to firmware it does not understand.

### 9. No pretending to understand arbitrary firmware

The application does **not** assume it can understand an arbitrary firmware binary simply
because that binary was loaded or flashed. Understanding comes from a trusted capability pack
plus facts read from the running board — never from the presence of a file.

## Consequences

- Supporting a new firmware release becomes a **data** change with its own review and test
  suite, not a core release.
- The safety-critical core stays small, stable and auditable.
- Pack signing, revocation and distribution infrastructure becomes a real deliverable
  (milestone M10), and must exist before packs are distributed rather than after.
- This architecture **does not** reduce what the program can do to an aircraft. It makes
  version and board coverage expandable without pushing volatile knowledge into the core.
