#!/usr/bin/env python3
"""Fail closed if a Pages artifact contains private or unexpected material."""

from __future__ import annotations

import argparse
import re
from pathlib import Path

ALLOWED_SUFFIXES = {".css", ".html", ".js", ".svg", ".wasm", ".webmanifest"}
REQUIRED_FILES = {
    "index.html",
    "manifest.webmanifest",
    "sw.js",
    "favicon.svg",
    "wasm/ade_web_storage_wasm_bridge.js",
    "wasm/ade_web_storage_wasm_bridge_bg.wasm",
    "wasm/ade_web_readonly_serial_wasm_bridge.js",
    "wasm/ade_web_readonly_serial_wasm_bridge_bg.wasm",
}
PRIVATE_PATTERNS = {
    "GitHub token": re.compile(r"(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,})"),
    "cloud access key": re.compile(r"\bAKIA[0-9A-Z]{16}\b"),
    "private key": re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
    "bearer credential": re.compile(r"(?i)authorization\s*:\s*bearer\s+[A-Za-z0-9._-]{20,}"),
    "GitHub runner secret context": re.compile(r"\b(?:GITHUB_TOKEN|ACTIONS_ID_TOKEN_REQUEST_TOKEN)\b"),
    "device identifier field": re.compile(r"\b(?:serialNumber|usbVendorId|usbProductId|deviceId)\b"),
    "captured protocol trace": re.compile(r"\b(?:rawMspBytes|mspHexDump|diagnosticTracePanel)\b", re.IGNORECASE),
}


def inspect(root: Path) -> list[str]:
    errors: list[str] = []
    if root.is_symlink() or not root.is_dir():
        return [f"artifact directory is missing or unsafe: {root}"]

    observed: set[str] = set()
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            errors.append(f"symbolic link is prohibited: {path.relative_to(root)}")
            continue
        if not path.is_file():
            continue
        relative = path.relative_to(root).as_posix()
        observed.add(relative)
        if path.suffix.lower() not in ALLOWED_SUFFIXES:
            errors.append(f"unexpected public artifact type: {relative}")
            continue
        if path.suffix.lower() == ".wasm":
            if path.read_bytes()[:4] != b"\x00asm":
                errors.append(f"invalid WASM binary: {relative}")
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            errors.append(f"public text asset is not UTF-8: {relative}")
            continue
        for label, pattern in PRIVATE_PATTERNS.items():
            if pattern.search(text):
                errors.append(f"{label} is prohibited in public artifact: {relative}")

    missing = sorted(REQUIRED_FILES - observed)
    if missing:
        errors.append(f"required public assets are missing: {missing}")
    if any(relative.endswith(".map") for relative in observed):
        errors.append("source maps are prohibited in the Pages artifact")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    errors = inspect(args.directory.resolve())
    if errors:
        print("PUBLIC WEB ARTIFACT GATE FAILED")
        for error in errors:
            print(f"  - {error}")
        return 1
    print("public Web artifact privacy gate passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
