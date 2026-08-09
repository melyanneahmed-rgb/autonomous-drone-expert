#!/usr/bin/env python3
"""Fail-closed Web Serial read-only authority and privacy gate (ADR-0013)."""

from __future__ import annotations

import hashlib
import re
import sys
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parent.parent
ADAPTER = PurePosixPath("web/src/transport/webserial-readonly-host.mjs")
DECLARATION = PurePosixPath("web/src/transport/webserial-readonly-host.d.mts")
SERIAL_BRIDGE = PurePosixPath("crates/web-readonly-serial-wasm-bridge/src/lib.rs")
STORAGE_BRIDGE = PurePosixPath("crates/web-storage-wasm-bridge/src/lib.rs")
EXPECTED_PACKAGE_LOCK_SHA256 = (
    "c3015e9454da094d307975921b8aa2c195a15b9dffe0498a9c758b57d922c05d"
)


def product_sources(root: Path = ROOT) -> dict[PurePosixPath, str]:
    files: dict[PurePosixPath, str] = {}
    for directory in (root / "web" / "src", root / "web" / "public"):
        if not directory.is_dir():
            continue
        for path in sorted(candidate for candidate in directory.rglob("*") if candidate.is_file()):
            files[PurePosixPath(path.relative_to(root).as_posix())] = path.read_text(
                encoding="utf-8"
            )
    return files


def source_authority_errors(files: dict[PurePosixPath, str]) -> list[str]:
    errors: list[str] = []
    adapter = files.get(ADAPTER)
    if adapter is None:
        return [f"missing designated Web Serial adapter: {ADAPTER}"]
    if DECLARATION not in files:
        errors.append(f"missing first-party Web Serial declaration: {DECLARATION}")

    serial_patterns = {
        "navigator.serial": re.compile(r"(?:globalThis\.)?navigator\?*\.serial"),
        "requestPort": re.compile(r"\brequestPort\b"),
        "writer.write": re.compile(r"#writer\.write\s*\("),
    }
    for label, pattern in serial_patterns.items():
        owners = [path for path, source in files.items() if pattern.search(source)]
        if owners != [ADAPTER]:
            errors.append(f"{label} authority must exist only in {ADAPTER}; found {owners}")

    if len(re.findall(r"\brequestPort\s*\(", adapter)) != 1:
        errors.append("requestPort must have exactly one call in the explicit selection boundary")
    if len(re.findall(r"#writer\.write\s*\(", adapter)) != 1:
        errors.append("writer.write must have exactly one call in the designated adapter")
    required = (
        "selectPortFromUserGesture",
        "open({ baudRate: INITIAL_MSP_BAUD_RATE })",
        "exchange-identification-read",
        "acceptReadChunk",
        "acceptExchangeFailure",
        "releaseLock",
        "close",
    )
    for marker in required:
        if marker not in adapter:
            errors.append(f"designated adapter is missing required bounded behavior: {marker}")

    forbidden_adapter = (
        (r"\bgetPorts\s*\(", "automatic/granted port enumeration"),
        (r"\b(sendRaw|writeRaw|sendMsp|writeCommand|executeArbitraryBytes)\b", "raw API"),
        (r"\b(CommandId|WriteApproval|TransportEffect|OutboundPacket)\b", "Rust authority type"),
        (r"\bMSP_[A-Z0-9_]*|\b(?:buildMsp|decodeMsp|encodeMsp)\b", "JavaScript MSP semantics"),
        (r"\b(console\.|localStorage|sessionStorage|indexedDB)\b", "logging/persistence"),
    )
    for pattern, label in forbidden_adapter:
        if re.search(pattern, adapter, re.IGNORECASE):
            errors.append(f"designated adapter contains forbidden {label}")

    all_source = "\n".join(files.values())
    if re.search(r"navigator\?*\.(?:usb|hid)\b|\b(?:WebUSB|WebHID|USBDevice|HIDDevice)\b", all_source):
        errors.append("WebUSB/WebHID authority is forbidden")

    outside = "\n".join(source for path, source in files.items() if path != ADAPTER)
    if re.search(
        r"(?:globalThis\.)?navigator\?*\.serial|\brequestPort\b|#writer\.write\s*\(|"
        r"\b(?:SerialPort|WriteApproval|TransportEffect|CommandId)\b",
        outside,
    ):
        errors.append("serial/MSP/write authority escaped the designated adapter")

    for path, source in files.items():
        if path == ADAPTER:
            continue
        if path.suffix in {".tsx", ".jsx"} or path.name == "sw.js":
            if re.search(r"\bserial\b|requestPort|writer\.write", source, re.IGNORECASE):
                errors.append(f"UI/service-worker serial authority forbidden at {path}")
    return errors


def repository_errors(root: Path = ROOT) -> list[str]:
    errors = source_authority_errors(product_sources(root))
    for relative in (SERIAL_BRIDGE, STORAGE_BRIDGE):
        if not (root / relative).is_file():
            errors.append(f"missing bridge source: {relative}")
    if (root / SERIAL_BRIDGE).is_file():
        serial_bridge = (root / SERIAL_BRIDGE).read_text(encoding="utf-8")
        if "#![forbid(unsafe_code)]" not in serial_bridge:
            errors.append("read-only serial bridge must forbid first-party unsafe")
        for marker in (
            "WasmReadonlySerialDiscovery",
            "MspV1ResponseAccumulator",
            "WriteCommandClass::NoWrite",
            "packet.approval().is_some()",
        ):
            if marker not in serial_bridge:
                errors.append(f"Rust serial authority proof missing: {marker}")
        directive_impl = re.search(
            r"impl WasmReadonlySerialDirective\s*\{(?P<body>.*?)^\}",
            serial_bridge,
            re.DOTALL | re.MULTILINE,
        )
        if directive_impl is None or "constructor" in directive_impl.group("body"):
            errors.append("JavaScript-visible directive constructor is forbidden")
    if (root / STORAGE_BRIDGE).is_file():
        storage_bridge = (root / STORAGE_BRIDGE).read_text(encoding="utf-8")
        if re.search(r"\b(TransportEffect|OutboundPacket|WebSerial|requestPort)\b", storage_bridge):
            errors.append("storage-only WASM bridge gained transport authority")

    lock = root / "web" / "package-lock.json"
    if not lock.is_file():
        errors.append("web/package-lock.json is missing")
    elif hashlib.sha256(lock.read_bytes()).hexdigest() != EXPECTED_PACKAGE_LOCK_SHA256:
        errors.append("web/package-lock.json drifted from the owner-approved SHA-256")
    return errors


def main() -> int:
    errors = repository_errors()
    if errors:
        print("WEB SERIAL AUTHORITY GATE FAILED\n")
        for error in errors:
            print(f"  - {error}")
        return 1
    print("web serial read-only authority gate passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
