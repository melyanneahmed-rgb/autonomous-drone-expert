#!/usr/bin/env python3
"""Stage the two audited generated WASM bridges for a production Web build."""

from __future__ import annotations

import argparse
import hashlib
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXPECTED_FILES = {
    "ade_web_storage_wasm_bridge.js": "storage",
    "ade_web_storage_wasm_bridge_bg.wasm": "storage",
    "ade_web_readonly_serial_wasm_bridge.js": "serial",
    "ade_web_readonly_serial_wasm_bridge_bg.wasm": "serial",
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def require_generated_file(path: Path) -> None:
    if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
        raise SystemExit(f"generated WASM asset is missing or unsafe: {path}")
    if path.suffix == ".wasm" and path.read_bytes()[:4] != b"\x00asm":
        raise SystemExit(f"generated WASM asset has an invalid magic header: {path}")
    if path.suffix == ".js" and b"http://" in path.read_bytes():
        raise SystemExit(f"generated WASM JavaScript contains an insecure URL: {path}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--storage-root", type=Path, default=ROOT / "target/storage-wasm-web")
    parser.add_argument("--serial-root", type=Path, default=ROOT / "target/webserial-wasm-web")
    parser.add_argument("--destination", type=Path, default=ROOT / "web/public/wasm")
    args = parser.parse_args()

    roots = {"storage": args.storage_root.resolve(), "serial": args.serial_root.resolve()}
    destination = args.destination.resolve()
    destination.mkdir(parents=True, exist_ok=True)

    staged: list[tuple[str, str]] = []
    for filename, source_group in EXPECTED_FILES.items():
        source = roots[source_group] / filename
        require_generated_file(source)
        target = destination / filename
        shutil.copyfile(source, target)
        require_generated_file(target)
        staged.append((filename, sha256(target)))

    unexpected = sorted(path.name for path in destination.iterdir() if path.name not in EXPECTED_FILES)
    if unexpected:
        raise SystemExit(f"unexpected generated WASM assets in destination: {unexpected}")

    for filename, digest in staged:
        print(f"{digest}  {filename}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
