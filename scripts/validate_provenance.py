#!/usr/bin/env python3
"""Provenance gate.

Enforces the source-provenance policy (ADR-0004, provenance/README.md):

  * every record matches provenance/schema.json,
  * every source is a pinned tag or commit -- never a moving branch,
  * no record admits copied material,
  * source state and verification state are recorded independently,
  * a verification state above NOT_REPRODUCED carries the date that earned it,
  * every MSP constant appearing in Rust source has a matching record.

A payload layout may be documented from a pinned source at any verification state.
Documenting a layout is not exercising it: what has actually been done with a fact is
carried by verification_state alone.

Standard library only.
"""

from __future__ import annotations

import json
import re
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PROV = ROOT / "provenance"
SCHEMA_PATH = PROV / "schema.json"
RECORDS_DIR = PROV / "records"

errors: list[str] = []


def fail(record: str, message: str) -> None:
    errors.append(f"{record}: {message}")


def type_ok(value, expected) -> bool:
    names = expected if isinstance(expected, list) else [expected]
    for name in names:
        if name == "string" and isinstance(value, str):
            return True
        if name == "integer" and isinstance(value, int) and not isinstance(value, bool):
            return True
        if name == "boolean" and isinstance(value, bool):
            return True
        if name == "object" and isinstance(value, dict):
            return True
        if name == "null" and value is None:
            return True
    return False


def check_object(record_name: str, prefix: str, value: dict, spec: dict) -> None:
    for key in spec.get("required", []):
        if key not in value:
            fail(record_name, f"missing required field '{prefix}{key}'")
    for key, sub in spec.get("properties", {}).items():
        if key not in value:
            continue
        actual = value[key]
        if "type" in sub and not type_ok(actual, sub["type"]):
            fail(
                record_name,
                f"field '{prefix}{key}' has wrong type: expected {sub['type']}, "
                f"got {type(actual).__name__}",
            )
            continue
        if "enum" in sub and actual not in sub["enum"]:
            fail(
                record_name,
                f"field '{prefix}{key}' value {actual!r} is not one of {sub['enum']}",
            )
        forbidden = sub.get("forbidden_values")
        if forbidden and isinstance(actual, str) and actual.lower() in {
            f.lower() for f in forbidden
        }:
            fail(
                record_name,
                f"field '{prefix}{key}' is '{actual}', which is a moving reference. "
                "Provenance must cite a pinned tag or commit.",
            )
        if isinstance(actual, dict) and "properties" in sub:
            check_object(record_name, f"{prefix}{key}.", actual, sub)


def check_invariants(record_name: str, rec: dict) -> None:
    verification_state = rec.get("verification_state")
    verification = rec.get("verification") or {}

    if rec.get("licensing", {}).get("copied_material") is not False:
        fail(
            record_name,
            "licensing.copied_material must be false. Copied material is prohibited "
            "(ADR-0004).",
        )

    # A payload layout may be documented from a pinned source at any verification state.
    # There is deliberately no rule coupling its presence to verification: doing so made
    # it impossible to document a layout before the code that would exercise it existed.

    if verification_state == "HARDWARE_OBSERVED" and not verification.get(
        "hardware_observed_at"
    ):
        fail(
            record_name,
            "HARDWARE_OBSERVED requires verification.hardware_observed_at",
        )

    if verification_state == "MOCK_EXERCISED" and not verification.get(
        "mock_exercised_at"
    ):
        fail(record_name, "MOCK_EXERCISED requires verification.mock_exercised_at")

    retrieved = rec.get("source", {}).get("retrieved_at")
    if isinstance(retrieved, str):
        try:
            date.fromisoformat(retrieved)
        except ValueError:
            fail(record_name, f"source.retrieved_at '{retrieved}' is not an ISO date")

    ref = rec.get("source", {}).get("ref")
    url = rec.get("source", {}).get("url")
    if isinstance(url, str) and isinstance(ref, str) and ref not in url:
        fail(
            record_name,
            f"source.url does not contain the pinned ref '{ref}'; the URL may point at a "
            "moving branch.",
        )


def check_code_constants(known_names: set[str]) -> None:
    crates = ROOT / "crates"
    if not crates.is_dir():
        return
    pattern = re.compile(r"\bMSP2?_[A-Z0-9_]+\b")
    for rs in sorted(crates.rglob("*.rs")):
        text = rs.read_text(encoding="utf-8")
        for name in sorted(set(pattern.findall(text))):
            if name not in known_names:
                errors.append(
                    f"{rs.relative_to(ROOT)}: protocol constant '{name}' appears in code "
                    "with no provenance record. Code follows records, never the reverse."
                )


def main() -> int:
    if not SCHEMA_PATH.is_file():
        print(f"missing schema: {SCHEMA_PATH}")
        return 1
    schema = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))

    if not RECORDS_DIR.is_dir():
        print(f"missing records directory: {RECORDS_DIR}")
        return 1

    records = sorted(RECORDS_DIR.glob("*.json"))
    if not records:
        print("no provenance records found")
        return 1

    seen_ids: set[str] = set()
    known_names: set[str] = set()
    verification_states: set[str] = set()

    for path in records:
        name = path.relative_to(ROOT).as_posix()
        try:
            rec = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            fail(name, f"invalid JSON: {exc}")
            continue
        if not isinstance(rec, dict):
            fail(name, "record must be a JSON object")
            continue
        check_object(name, "", rec, schema)
        check_invariants(name, rec)
        rid = rec.get("record_id")
        if rid in seen_ids:
            fail(name, f"duplicate record_id '{rid}'")
        if isinstance(rid, str):
            seen_ids.add(rid)
        if isinstance(rec.get("name"), str):
            known_names.add(rec["name"])
        if isinstance(rec.get("verification_state"), str):
            verification_states.add(rec["verification_state"])

    check_code_constants(known_names)

    if errors:
        print("PROVENANCE GATE FAILED\n")
        for item in errors:
            print(f"  - {item}")
        return 1

    states = sorted({r for r in verification_states})
    print(
        f"provenance gate passed ({len(records)} records, all pinned; "
        f"verification states: {', '.join(states)})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
