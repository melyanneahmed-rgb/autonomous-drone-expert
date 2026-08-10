#!/usr/bin/env python3
"""Verify committed Web Serial WASM product assets and canonical regeneration."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = Path("policy/webserial-wasm-assets.json")
OUTPUT_DIRECTORY = Path("web/public/wasm")
EXPECTED_OUTPUT_NAMES = {
    "ade_web_readonly_serial_wasm_bridge.js",
    "ade_web_readonly_serial_wasm_bridge_bg.wasm",
}
HEX_SHA256 = re.compile(r"^[0-9a-f]{64}$")
HEX_SHA1 = re.compile(r"^[0-9a-f]{40}$")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def git_blob_sha1(path: Path) -> str:
    content = path.read_bytes()
    header = f"blob {len(content)}\0".encode()
    return hashlib.sha1(header + content, usedforsecurity=False).hexdigest()


def _load_manifest(path: Path) -> tuple[dict, list[str]]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        return {}, [f"cannot read asset provenance manifest: {error}"]
    if not isinstance(value, dict):
        return {}, ["asset provenance manifest must be a JSON object"]
    return value, []


def _check_record(root: Path, record: object, label: str) -> list[str]:
    if not isinstance(record, dict):
        return [f"{label} must be an object"]
    path_value = record.get("path")
    expected_sha = record.get("sha256")
    expected_blob = record.get("git_blob_sha1")
    if not isinstance(path_value, str) or not path_value:
        return [f"{label}.path must be a non-empty string"]
    path = root / path_value
    errors: list[str] = []
    if not path.is_file():
        return [f"missing {label}: {path_value}"]
    if not isinstance(expected_sha, str) or not HEX_SHA256.fullmatch(expected_sha):
        errors.append(f"{label}.sha256 must be lowercase SHA-256")
    elif sha256(path) != expected_sha:
        errors.append(f"{label} SHA-256 drift: {path_value}")
    if not isinstance(expected_blob, str) or not HEX_SHA1.fullmatch(expected_blob):
        errors.append(f"{label}.git_blob_sha1 must be lowercase Git SHA-1")
    elif git_blob_sha1(path) != expected_blob:
        errors.append(f"{label} Git blob drift: {path_value}")
    return errors


def verify(
    root: Path = ROOT,
    generated_dir: Path | None = None,
    input_wasm: Path | None = None,
    manifest_path: Path | None = None,
) -> list[str]:
    manifest_file = manifest_path or root / MANIFEST
    manifest, errors = _load_manifest(manifest_file)
    if errors:
        return errors

    if manifest.get("schema_version") != 1:
        errors.append("asset provenance schema_version must be 1")
    if manifest.get("classification") != "derived-build-output-no-new-authority":
        errors.append("generated assets must be classified as derived output without authority")

    source = manifest.get("source")
    generator = manifest.get("generator")
    if not isinstance(source, dict) or not isinstance(generator, dict):
        return errors + ["source and generator provenance must be objects"]
    if source.get("accepted_main_commit") != "8ef20be74a34912de53030d28d29b5e4108ddd08":
        errors.append("generated asset source baseline drifted")
    if source.get("package") != "ade-web-readonly-serial-wasm-bridge":
        errors.append("generated asset source package drifted")
    if source.get("build_toolchain") != "1.85.0":
        errors.append("product WASM build toolchain drifted")
    if source.get("target") != "wasm32-unknown-unknown":
        errors.append("product WASM target drifted")
    input_sha = source.get("input_wasm_sha256")
    if not isinstance(input_sha, str) or not HEX_SHA256.fullmatch(input_sha):
        errors.append("input_wasm_sha256 must be lowercase SHA-256")
    elif input_wasm is not None:
        if not input_wasm.is_file():
            errors.append(f"canonical input WASM is missing: {input_wasm}")
        elif sha256(input_wasm) != input_sha:
            errors.append("canonical input WASM SHA-256 drift")
    errors.extend(_check_record(root, source.get("manifest"), "source.manifest"))
    errors.extend(_check_record(root, source.get("rust_source"), "source.rust_source"))

    if generator.get("package") != "wasm-bindgen-cli-support":
        errors.append("generator package drifted")
    if generator.get("version") != "0.2.127":
        errors.append("generator version must remain exactly 0.2.127")
    if generator.get("toolchain") != "1.97.1":
        errors.append("isolated generator toolchain drifted")
    if generator.get("canonical_environment") != "github-actions/ubuntu-latest":
        errors.append("canonical generation must remain on trusted Linux CI")
    errors.extend(
        _check_record(root, generator.get("isolated_manifest"), "generator.isolated_manifest")
    )
    errors.extend(_check_record(root, generator.get("isolated_lock"), "generator.isolated_lock"))

    outputs = manifest.get("outputs")
    if not isinstance(outputs, list) or len(outputs) != 2:
        return errors + ["exactly two generated product outputs are required"]
    output_records: dict[str, dict] = {}
    for index, record in enumerate(outputs):
        if not isinstance(record, dict):
            errors.append(f"outputs[{index}] must be an object")
            continue
        path_value = record.get("path")
        expected_sha = record.get("sha256")
        expected_size = record.get("size")
        if not isinstance(path_value, str):
            errors.append(f"outputs[{index}].path must be a string")
            continue
        name = Path(path_value).name
        output_records[name] = record
        if Path(path_value).parent != OUTPUT_DIRECTORY:
            errors.append(f"generated output escaped {OUTPUT_DIRECTORY.as_posix()}: {path_value}")
        committed = root / path_value
        if not committed.is_file():
            errors.append(f"missing committed generated output: {path_value}")
            continue
        if not isinstance(expected_sha, str) or not HEX_SHA256.fullmatch(expected_sha):
            errors.append(f"invalid output SHA-256: {path_value}")
        elif sha256(committed) != expected_sha:
            errors.append(f"committed generated output SHA-256 drift: {path_value}")
        if not isinstance(expected_size, int) or committed.stat().st_size != expected_size:
            errors.append(f"committed generated output size drift: {path_value}")

    if set(output_records) != EXPECTED_OUTPUT_NAMES:
        errors.append("generated product output allowlist drifted")
    committed_names = {
        path.name for path in (root / OUTPUT_DIRECTORY).iterdir() if path.is_file()
    } if (root / OUTPUT_DIRECTORY).is_dir() else set()
    if committed_names != EXPECTED_OUTPUT_NAMES:
        errors.append(f"committed generated directory has unexpected entries: {sorted(committed_names)}")

    if generated_dir is not None:
        generated_names = {
            path.name for path in generated_dir.iterdir() if path.is_file()
        } if generated_dir.is_dir() else set()
        if generated_names != EXPECTED_OUTPUT_NAMES:
            errors.append(f"canonical regeneration has unexpected entries: {sorted(generated_names)}")
        for name in sorted(EXPECTED_OUTPUT_NAMES):
            generated = generated_dir / name
            committed = root / OUTPUT_DIRECTORY / name
            if not generated.is_file() or not committed.is_file():
                continue
            record = output_records.get(name, {})
            if generated.read_bytes() != committed.read_bytes():
                errors.append(f"byte-for-byte regeneration drift: {name}")
            expected_sha = record.get("sha256")
            if isinstance(expected_sha, str) and sha256(generated) != expected_sha:
                errors.append(f"canonical regenerated SHA-256 drift: {name}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--generated-dir", type=Path)
    parser.add_argument("--input-wasm", type=Path)
    args = parser.parse_args()
    errors = verify(generated_dir=args.generated_dir, input_wasm=args.input_wasm)
    if errors:
        print("WEB SERIAL PRODUCT ASSET GATE FAILED\n")
        for error in errors:
            print(f"  - {error}")
        return 1
    suffix = " + byte-for-byte regeneration" if args.generated_dir else ""
    print(f"web serial product asset provenance gate passed{suffix}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
