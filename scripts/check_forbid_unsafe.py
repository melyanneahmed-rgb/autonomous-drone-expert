#!/usr/bin/env python3
"""Unsafe-Rust gate.

Every first-party product crate and the isolated first-party WASM build tool must declare
`#![forbid(unsafe_code)]`, and no `unsafe` token may appear anywhere in first-party Rust.
No exception has been proven necessary yet, including in the transport crate. Relaxing this
is a dedicated pull request with written justification and owner review -- never a quiet edit.

Standard library only.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES = ROOT / "crates"
DECLARATION = "#![forbid(unsafe_code)]"
ISOLATED_TOOL_MAIN = Path("tools/wasm-bindgen-cli-support/src/main.rs")


def require_declaration(root: Path, source: Path, label: str, errors: list[str]) -> None:
    path = root / source
    if not path.is_file():
        errors.append(f"{label}: missing {source.as_posix()}")
        return
    if DECLARATION not in path.read_text(encoding="utf-8"):
        errors.append(f"{label}: {source.as_posix()} does not declare {DECLARATION}")


def check_repository(root: Path) -> tuple[list[str], int]:
    errors: list[str] = []
    crates = root / "crates"
    if not crates.is_dir():
        errors.append("missing crates/ directory")
        crate_dirs: list[Path] = []
    else:
        crate_dirs = sorted(path for path in crates.iterdir() if path.is_dir())
        if not crate_dirs:
            errors.append("crates/ exists but contains no crate.")

    for crate in crate_dirs:
        require_declaration(
            root,
            crate.relative_to(root) / "src" / "lib.rs",
            crate.name,
            errors,
        )

    require_declaration(
        root,
        ISOLATED_TOOL_MAIN,
        "isolated WASM build tool",
        errors,
    )

    for rs in sorted(root.rglob("*.rs")):
        if ".git" in rs.parts or "target" in rs.parts:
            continue
        text = rs.read_text(encoding="utf-8")
        stripped = text.replace("forbid(unsafe_code)", "")
        if re.search(r"\bunsafe\b", stripped):
            errors.append(
                f"{rs.relative_to(root)}: contains an 'unsafe' token. Not permitted in this "
                "stage."
            )

    return errors, len(crate_dirs)


def main() -> int:
    errors, crate_count = check_repository(ROOT)
    if errors:
        print("UNSAFE-RUST GATE FAILED\n")
        for item in errors:
            print(f"  - {item}")
        return 1

    print(f"unsafe-rust gate passed ({crate_count} crates + 1 isolated tool)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
