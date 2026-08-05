#!/usr/bin/env python3
"""Local secret-pattern scan.

A conservative, best-effort scan for credentials committed by accident. It is NOT a
replacement for GitHub secret scanning with push protection, which is a repository
setting rather than a CI step and is not assumed to be available on this plan.

Standard library only.
"""

from __future__ import annotations

import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SELF = Path(__file__).resolve()

# Patterns are assembled at runtime so that this file never matches itself.
PATTERNS = [
    ("GitHub personal access token", re.compile("gh" + r"[pousr]_[A-Za-z0-9]{30,}")),
    ("GitHub fine-grained token", re.compile("github" + r"_pat_[A-Za-z0-9_]{30,}")),
    ("AWS access key id", re.compile(r"\b" + "AKIA" + r"[0-9A-Z]{16}\b")),
    ("Slack token", re.compile("xox" + r"[abprs]-[A-Za-z0-9-]{10,}")),
    ("Private key block", re.compile("-----BEGIN [A-Z ]*PRIVATE" + " KEY-----")),
    ("Generic bearer secret", re.compile(r"(?i)\bauthorization\s*:\s*bearer\s+[A-Za-z0-9._-]{20,}")),
]

TEXT_SUFFIXES = {
    ".rs", ".toml", ".md", ".json", ".yml", ".yaml", ".py", ".ts", ".tsx", ".js",
    ".jsx", ".txt", ".sh", ".cfg", ".ini", ".env", "",
}

findings: list[str] = []

out = subprocess.run(
    ["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True
)
files = [ROOT / line for line in out.stdout.splitlines() if line]

scanned = 0
for path in files:
    if path.resolve() == SELF:
        continue
    if path.suffix.lower() not in TEXT_SUFFIXES:
        continue
    try:
        text = path.read_text(encoding="utf-8")
    except (UnicodeDecodeError, OSError):
        continue
    scanned += 1
    for label, pattern in PATTERNS:
        for match in pattern.finditer(text):
            line = text[: match.start()].count("\n") + 1
            findings.append(f"{path.relative_to(ROOT)}:{line}: possible {label}")

if findings:
    print("SECRET SCAN FAILED\n")
    for item in findings:
        print(f"  - {item}")
    raise SystemExit(1)

print(f"secret scan passed ({scanned} text files scanned)")
