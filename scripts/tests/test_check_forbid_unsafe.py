"""Regression tests for the first-party unsafe-Rust gate."""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import check_forbid_unsafe  # noqa: E402


class UnsafeRustGateTests(unittest.TestCase):
    def fixture(self, tool_source: str, crate_source: str | None = None) -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        crate = root / "crates" / "sample" / "src"
        crate.mkdir(parents=True)
        crate.joinpath("lib.rs").write_text(
            crate_source or f"{check_forbid_unsafe.DECLARATION}\n",
            encoding="utf-8",
        )
        tool = root / check_forbid_unsafe.ISOLATED_TOOL_MAIN
        tool.parent.mkdir(parents=True)
        tool.write_text(tool_source, encoding="utf-8")
        return root

    def test_isolated_tool_without_compiler_declaration_fails(self) -> None:
        errors, _ = check_forbid_unsafe.check_repository(self.fixture("fn main() {}\n"))
        self.assertTrue(any("isolated WASM build tool" in error for error in errors))

    def test_isolated_tool_with_compiler_declaration_passes(self) -> None:
        root = self.fixture(f"{check_forbid_unsafe.DECLARATION}\nfn main() {{}}\n")
        self.assertEqual(check_forbid_unsafe.check_repository(root), ([], 1))

    def test_product_crate_without_compiler_declaration_still_fails(self) -> None:
        root = self.fixture(
            f"{check_forbid_unsafe.DECLARATION}\nfn main() {{}}\n",
            "pub fn product() {}\n",
        )
        errors, _ = check_forbid_unsafe.check_repository(root)
        self.assertTrue(any("sample" in error for error in errors))

    def test_unsafe_tokens_remain_rejected(self) -> None:
        root = self.fixture(
            f"{check_forbid_unsafe.DECLARATION}\nfn main() {{}}\n",
            f"{check_forbid_unsafe.DECLARATION}\nunsafe fn forbidden() {{}}\n",
        )
        errors, _ = check_forbid_unsafe.check_repository(root)
        self.assertTrue(any("contains an 'unsafe' token" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
