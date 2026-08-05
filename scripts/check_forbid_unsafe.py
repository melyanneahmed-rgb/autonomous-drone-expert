#!/usr/bin/env python3
"""Unsafe-Rust gate.

Every crate must declare `#![forbid(unsafe_code)]`, and no `unsafe` token may appear
anywhere in the workspace. No exception has been proven necessary yet, including in the
transport crate. Relaxing this is a dedicated pull request with written justification and
owner review -- never a quiet edit.

Standard library only.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES = ROOT / "crates"
DECLARATION = "#![forbid(unsafe_code)]"

errors: list[str] = []

if not CRATES.is_dir():
    print("no crates directory; nothing to check")
    raise SystemExit(0)

crate_dirs = sorted(p for p in CRATES.iterdir() if p.is_dir())
if not crate_dirs:
    errors.append("crates/ exists but contains no crate.")

for crate in crate_dirs:
    lib = crate / "src" / "lib.rs"
    if not lib.is_file():
        errors.append(f"{crate.name}: missing src/lib.rs")
        continue
    if DECLARATION not in lib.read_text(encoding="utf-8"):
        errors.append(f"{crate.name}: src/lib.rs does not declare {DECLARATION}")

for rs in sorted(ROOT.rglob("*.rs")):
    if ".git" in rs.parts or "target" in rs.parts:
        continue
    text = rs.read_text(encoding="utf-8")
    stripped = text.replace("forbid(unsafe_code)", "")
    if re.search(r"\bunsafe\b", stripped):
        errors.append(
            f"{rs.relative_to(ROOT)}: contains an 'unsafe' token. Not permitted in this "
            "stage."
        )

if errors:
    print("UNSAFE-RUST GATE FAILED\n")
    for item in errors:
        print(f"  - {item}")
    raise SystemExit(1)

print(f"unsafe-rust gate passed ({len(crate_dirs)} crates)")
