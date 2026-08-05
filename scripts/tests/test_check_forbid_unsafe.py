"""Regression tests for the unsafe-Rust gate.

The gate must detect `unsafe` in code and must not fire on prose. Firing on prose
punishes documentation, which is how a rule quietly stops being written about.

Note: the comment-stripping under test is a spike-branch experiment REJECTED FOR
PRODUCTION (see the module docstring in check_forbid_unsafe.py). The last test below
documents its known miss -- a `//` inside a string literal hides the rest of the line --
as an executable statement of the limitation rather than a claim of safety.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import check_forbid_unsafe as gate  # noqa: E402


class StripCommentsTests(unittest.TestCase):
    def assert_flagged(self, source: str, expected: bool) -> None:
        import re

        stripped = gate.strip_comments(source).replace("forbid(unsafe_code)", "")
        found = bool(re.search(r"\bunsafe\b", stripped))
        self.assertEqual(found, expected, f"source: {source!r}")

    def test_doc_comment_mentioning_unsafe_is_not_flagged(self) -> None:
        self.assert_flagged("//! If a candidate forced unsafe into our code.\n", False)

    def test_line_comment_mentioning_unsafe_is_not_flagged(self) -> None:
        self.assert_flagged("let x = 1; // unsafe would be needed here\n", False)

    def test_block_comment_mentioning_unsafe_is_not_flagged(self) -> None:
        self.assert_flagged("/* unsafe\n   across lines */\nlet x = 1;\n", False)

    def test_real_unsafe_block_is_flagged(self) -> None:
        self.assert_flagged("fn f() { unsafe { g() } }\n", True)

    def test_unsafe_fn_is_flagged(self) -> None:
        self.assert_flagged("pub unsafe fn f() {}\n", True)

    def test_unsafe_impl_is_flagged(self) -> None:
        self.assert_flagged("unsafe impl Send for T {}\n", True)

    def test_declaration_alone_is_not_flagged(self) -> None:
        self.assert_flagged("#![forbid(unsafe_code)]\n", False)

    def test_unsafe_after_a_comment_on_the_same_line_is_still_flagged(self) -> None:
        self.assert_flagged("unsafe { f() } // note\n", True)

    def test_known_limitation_slash_slash_in_string_literal_hides_the_line_tail(self) -> None:
        # DOCUMENTED MISS, not a feature: the regex treats "//" inside a string literal
        # as a comment, so an unsafe token later on the same line escapes this scan.
        # The compiler-level forbid(unsafe_code) declaration still rejects it at build
        # time. This is exactly why the approach is rejected for production.
        self.assert_flagged('let s = "https://x"; unsafe { f() }\n', False)


if __name__ == "__main__":
    unittest.main()
