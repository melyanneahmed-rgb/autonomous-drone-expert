"""Regression tests for the isolation gate.

Standard library only. Run with:

    python3 -m unittest discover -s scripts/tests

The path containment test exists because the original implementation compared strings:
`str(resolved).startswith(str(ROOT))`. That accepts a sibling directory whose name merely
shares a prefix with the repository, which is exactly how an external dependency would be
smuggled in.
"""

from __future__ import annotations

import os
import sys
import tempfile
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


class DependencyPolicyTests(unittest.TestCase):
    """Fixture-based tests for the first-party workspace-path dependency policy (ADR-0009).

    Each test builds a throwaway workspace in a temporary directory; no real repository file
    is ever touched. The dependant crate is `crates/beta`; the dependee is `crates/alpha`
    (package `ade-alpha`).
    """

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.members = ["crates/*"]
        (self.root / "Cargo.toml").write_text(
            '[workspace]\nmembers = ["crates/*"]\n', encoding="utf-8"
        )
        self._make_crate("crates/alpha", "ade-alpha")
        self._make_crate("crates/beta", "ade-beta")
        self.beta_manifest = self.root / "crates" / "beta" / "Cargo.toml"

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _make_crate(self, rel: str, name: str) -> None:
        crate = self.root / rel
        (crate / "src").mkdir(parents=True, exist_ok=True)
        (crate / "Cargo.toml").write_text(
            f'[package]\nname = "{name}"\nversion = "0.0.0"\n', encoding="utf-8"
        )

    def classify(self, dep_name: str, spec):
        return check_isolation.classify_dependency(
            dep_name, spec, self.beta_manifest, self.root, self.members
        )

    # ---- PASS ----
    def test_valid_internal_path_dependency_passes(self) -> None:
        self.assertIsNone(self.classify("ade-alpha", {"path": "../alpha"}))

    def test_valid_renamed_path_dependency_with_matching_package_passes(self) -> None:
        self.assertIsNone(
            self.classify("myalpha", {"path": "../alpha", "package": "ade-alpha"})
        )

    def test_dev_and_target_tables_are_covered_and_accept_valid_paths(self) -> None:
        manifest = {
            "dev-dependencies": {"ade-alpha": {"path": "../alpha"}},
            "target": {"cfg(windows)": {"dependencies": {"ade-alpha": {"path": "../alpha"}}}},
        }
        tables = dict(check_isolation._iter_dependency_tables(manifest))
        self.assertIn("dev-dependencies", tables)
        self.assertIn("target.dependencies", tables)
        for _name, table in check_isolation._iter_dependency_tables(manifest):
            for name, spec in table.items():
                self.assertIsNone(self.classify(name, spec))

    # ---- FAIL ----
    def test_path_escaping_repository_is_rejected(self) -> None:
        self.assertIsNotNone(self.classify("evil", {"path": "../../../elsewhere"}))

    def test_path_inside_repo_but_not_a_member_is_rejected(self) -> None:
        (self.root / "extra").mkdir()
        (self.root / "extra" / "Cargo.toml").write_text(
            '[package]\nname = "ade-extra"\nversion = "0.0.0"\n', encoding="utf-8"
        )
        msg = self.classify("ade-extra", {"path": "../../extra"})
        self.assertIsNotNone(msg)
        self.assertIn("not a member", msg)

    def test_git_dependency_is_rejected(self) -> None:
        self.assertIsNotNone(self.classify("x", {"git": "https://example.invalid/x"}))

    def test_registry_version_dependency_is_rejected(self) -> None:
        self.assertIsNotNone(self.classify("serde", "1.0"))
        self.assertIsNotNone(self.classify("serde", {"version": "1.0"}))

    def test_path_plus_version_hybrid_is_rejected(self) -> None:
        msg = self.classify("ade-alpha", {"path": "../alpha", "version": "1.0"})
        self.assertIsNotNone(msg)
        self.assertIn("Hybrid", msg)

    def test_path_plus_git_is_rejected(self) -> None:
        self.assertIsNotNone(
            self.classify("ade-alpha", {"path": "../alpha", "git": "https://x.invalid"})
        )

    def test_wildcard_version_is_rejected(self) -> None:
        self.assertIsNotNone(self.classify("anything", "*"))
        self.assertIsNotNone(self.classify("anything", {"path": "*"}))

    def test_workspace_inherited_dependency_is_rejected(self) -> None:
        self.assertIsNotNone(self.classify("ade-alpha", {"workspace": True}))

    def test_alias_hiding_a_different_package_is_rejected(self) -> None:
        # dep key 'ade-alpha' but the path resolves to package 'ade-beta'.
        msg = self.classify("ade-alpha", {"path": "../beta"})
        self.assertIsNotNone(msg)
        self.assertIn("hiding a different package", msg)

    def test_malformed_target_manifest_is_rejected(self) -> None:
        (self.root / "crates" / "alpha" / "Cargo.toml").write_text(
            "this is = = not valid toml [[", encoding="utf-8"
        )
        self.assertIsNone(check_isolation.read_package_name(
            self.root / "crates" / "alpha" / "Cargo.toml"
        ))
        self.assertIsNotNone(self.classify("ade-alpha", {"path": "../alpha"}))

    def test_symlink_escaping_the_repository_is_rejected(self) -> None:
        link = self.root / "crates" / "escape"
        try:
            os.symlink("/etc", link)
        except (OSError, NotImplementedError):
            self.skipTest("symlinks unsupported in this environment")
        # resolve() follows the symlink to /etc, which is outside the repository root.
        self.assertIsNotNone(self.classify("etc", {"path": "../escape"}))


if __name__ == "__main__":
    unittest.main()
