from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from check_public_web_artifact import REQUIRED_FILES, inspect  # noqa: E402


class PublicWebArtifactTests(unittest.TestCase):
    def fixture(self) -> tempfile.TemporaryDirectory[str]:
        temporary = tempfile.TemporaryDirectory()
        root = Path(temporary.name)
        for relative in REQUIRED_FILES:
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"\x00asm\x01\x00\x00\x00" if path.suffix == ".wasm" else b"safe")
        return temporary

    def test_minimal_expected_artifact_passes(self) -> None:
        with self.fixture() as directory:
            self.assertEqual(inspect(Path(directory)), [])

    def test_secret_device_id_and_unexpected_file_fail(self) -> None:
        with self.fixture() as directory:
            root = Path(directory)
            (root / "index.html").write_text("serialNumber github_pat_" + "A" * 35, encoding="utf-8")
            (root / "private.log").write_text("log", encoding="utf-8")
            errors = inspect(root)
            self.assertTrue(any("GitHub token" in error for error in errors))
            self.assertTrue(any("device identifier" in error for error in errors))
            self.assertTrue(any("unexpected public artifact type" in error for error in errors))

    def test_missing_asset_and_invalid_wasm_fail(self) -> None:
        with self.fixture() as directory:
            root = Path(directory)
            (root / "favicon.svg").unlink()
            wasm = root / "wasm/ade_web_storage_wasm_bridge_bg.wasm"
            wasm.write_bytes(b"not-wasm")
            errors = inspect(root)
            self.assertTrue(any("missing" in error for error in errors))
            self.assertTrue(any("invalid WASM" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
