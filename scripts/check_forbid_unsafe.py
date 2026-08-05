#!/usr/bin/env python3
"""Unsafe-Rust gate.

Every crate must declare `#![forbid(unsafe_code)]`, and no `unsafe` token may appear
anywhere in the workspace. No exception has been proven necessary yet, including in the
transport crate. Relaxing this is a dedicated pull request with written justification and
owner review -- never a quiet edit.

Standard library only.

SPIKE-BRANCH EXPERIMENT -- comment stripping is REJECTED FOR PRODUCTION
-----------------------------------------------------------------------
On this branch only, the token scan strips comments first so that prose *about* unsafe
(doc comments, design notes) does not fail the gate. That stripping is regex-based and is
NOT reliable protection: a `//` inside a string literal is treated as a comment start,
which deletes the remainder of that line and can hide a real `unsafe` token appearing
later on it. Example the regex gets wrong:

    let s = "https://x"; unsafe { f() }   // remainder after "//" vanishes from the scan

Because of that failure mode, this modification must NOT be promoted to `main` as the
production gate. The reliable layers remain: (1) the mandatory `#![forbid(unsafe_code)]`
declaration in every crate, which makes the *compiler* reject unsafe code regardless of
what this script sees, and (2) human review. Any future production improvement to this
scanner needs a proper tokenizer/parser, or compiler-level enforcement alone, in its own
reviewed pull request.
"""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES = ROOT / "crates"
DECLARATION = "#![forbid(unsafe_code)]"

_BLOCK_COMMENT = re.compile(r"/\*.*?\*/", re.DOTALL)
_LINE_COMMENT = re.compile(r"//[^\n]*")


def strip_comments(source: str) -> str:
    """Remove Rust comments so the gate scans code, not prose. EXPERIMENTAL.

    Without this, any file that merely *discusses* unsafe code -- a doc comment, a design
    note, a module header -- fails the gate, which teaches people to avoid writing about
    the rule.

    This implementation is regex-based and therefore NOT reliable: a `//` inside a string
    literal is treated as a comment start and hides the rest of that line from the scan,
    including any real `unsafe` token after it. That is a genuine missed-detection path.
    It is tolerated on the spike branch only because the compiler-enforced
    `#![forbid(unsafe_code)]` declaration -- which this script independently verifies is
    present in every crate -- rejects unsafe code regardless of this scan. REJECTED FOR
    PRODUCTION; see the module docstring.
    """
    return _LINE_COMMENT.sub("", _BLOCK_COMMENT.sub("", source))

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
    stripped = strip_comments(text).replace("forbid(unsafe_code)", "")
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
