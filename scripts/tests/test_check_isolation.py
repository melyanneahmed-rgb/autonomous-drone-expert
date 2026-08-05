"""Regression tests for the isolation gate.

Standard library only. Run with:

    python3 -m unittest discover -s scripts/tests

The path containment test exists because the original implementation compared strings:
`str(resolved).startswith(str(ROOT))`. That accepts a sibling directory whose name merely
shares a prefix with the repository, which is exactly how an external dependency would be
smuggled in.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import check_isolation  # noqa: E402


class PathEscapesRepositoryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.root = Path("/workspace/repo")
        self.manifest = self.root / "crates" / "alpha" / "Cargo.toml"

    def test_dependency_inside_repository_is_accepted(self) -> None:
        self.assertFalse(
            check_isolation.path_escapes_repository(self.manifest, "../beta", self.root)
        )

    def test_dependency_escaping_to_parent_is_rejected(self) -> None:
        self.assertTrue(
            check_isolation.path_escapes_repository(
                self.manifest, "../../../elsewhere", self.root
            )
        )

    def test_sibling_with_shared_prefix_is_rejected(self) -> None:
        # "/workspace/repo-malicious" starts with "/workspace/repo" as a string but is
        # not inside it as a path.
        self.assertTrue(
            check_isolation.path_escapes_repository(
                self.manifest, "../../../repo-malicious/crate", self.root
            )
        )

    def test_absolute_path_outside_repository_is_rejected(self) -> None:
        self.assertTrue(
            check_isolation.path_escapes_repository(self.manifest, "/etc", self.root)
        )

    def test_repository_root_itself_is_accepted(self) -> None:
        self.assertFalse(
            check_isolation.path_escapes_repository(self.manifest, "../..", self.root)
        )


if __name__ == "__main__":
    unittest.main()
