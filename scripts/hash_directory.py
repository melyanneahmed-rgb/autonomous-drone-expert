#!/usr/bin/env python3
"""Hash a directory as sorted relative-path, size and file-digest records."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    root = args.directory.resolve()
    if root.is_symlink() or not root.is_dir():
        raise SystemExit(f"artifact directory is missing or unsafe: {root}")

    records = []
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            raise SystemExit(f"artifact contains a symbolic link: {path}")
        if not path.is_file():
            continue
        records.append(
            {
                "path": path.relative_to(root).as_posix(),
                "size": path.stat().st_size,
                "sha256": file_sha256(path),
            }
        )
    if not records:
        raise SystemExit("artifact directory is empty")
    payload = "".join(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n" for record in records)
    print(hashlib.sha256(payload.encode("utf-8")).hexdigest())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
