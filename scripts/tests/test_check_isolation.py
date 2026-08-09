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
import shutil
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
        (self.root / "Cargo.toml").write_text(
            '[workspace]\nmembers = ["crates/*"]\n', encoding="utf-8"
        )
        self._make_crate("crates/alpha", "ade-alpha")
        self._make_crate("crates/beta", "ade-beta")
        self.beta_manifest = self.root / "crates" / "beta" / "Cargo.toml"
        # The actual member directories, as cargo metadata would report them. These unit
        # tests pass the set directly (fast); the cargo-metadata resolution itself is
        # covered end-to-end in WorkspaceMembershipViaCargoMetadataTests.
        self.member_dirs = {
            (self.root / "crates" / "alpha").resolve(),
            (self.root / "crates" / "beta").resolve(),
        }

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _make_crate(self, rel: str, name: str) -> None:
        crate = self.root / rel
        (crate / "src").mkdir(parents=True, exist_ok=True)
        (crate / "Cargo.toml").write_text(
            f'[package]\nname = "{name}"\nversion = "0.0.0"\n', encoding="utf-8"
        )

    def classify(self, dep_name: str, spec, table_name: str = "dependencies"):
        return check_isolation.classify_dependency(
            dep_name,
            spec,
            self.beta_manifest,
            self.root,
            self.member_dirs,
            table_name,
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
        self.assertIn("not an actual member", msg)

    def test_git_dependency_is_rejected(self) -> None:
        self.assertIsNotNone(self.classify("x", {"git": "https://example.invalid/x"}))

    def test_registry_version_dependency_is_rejected(self) -> None:
        self.assertIsNotNone(self.classify("serde", "1.0"))
        self.assertIsNotNone(self.classify("serde", {"version": "1.0"}))

    def test_exact_wasm_bindgen_runtime_exception_is_location_and_shape_bound(self) -> None:
        manifest = self.root / "crates" / "web-storage-wasm-bridge" / "Cargo.toml"
        spec = {
            "version": "=0.2.127",
            "default-features": False,
            "features": ["std"],
        }
        self.assertIsNone(
            check_isolation.classify_dependency(
                "wasm-bindgen", spec, manifest, self.root, self.member_dirs
            )
        )
        drift = {**spec, "version": "=0.2.126"}
        self.assertIsNotNone(
            check_isolation.classify_dependency(
                "wasm-bindgen", drift, manifest, self.root, self.member_dirs
            )
        )
        self.assertIsNotNone(
            check_isolation.classify_dependency(
                "wasm-bindgen",
                spec,
                self.beta_manifest,
                self.root,
                self.member_dirs,
            )
        )
        self.assertIsNotNone(
            check_isolation.classify_dependency(
                "wasm-bindgen",
                spec,
                manifest,
                self.root,
                self.member_dirs,
                "dev-dependencies",
            )
        )

    def test_exact_cli_support_exception_is_tooling_only(self) -> None:
        manifest = self.root / "tools" / "wasm-bindgen-cli-support" / "Cargo.toml"
        spec = {"version": "=0.2.127"}
        self.assertIsNone(
            check_isolation.classify_dependency(
                "wasm-bindgen-cli-support",
                spec,
                manifest,
                self.root,
                self.member_dirs,
            )
        )
        self.assertIsNotNone(
            check_isolation.classify_dependency(
                "wasm-bindgen-cli-support",
                spec,
                self.beta_manifest,
                self.root,
                self.member_dirs,
            )
        )

    def test_full_wasm_bindgen_cli_is_always_rejected(self) -> None:
        self.assertIsNotNone(
            self.classify("wasm-bindgen-cli", {"version": "=0.2.127"})
        )

    def test_alias_cannot_hide_an_audited_registry_package(self) -> None:
        self.assertIsNotNone(
            self.classify(
                "binding-tool",
                {"package": "wasm-bindgen", "version": "=0.2.127"},
            )
        )

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

    def test_path_dependency_fails_closed_when_membership_unknown(self) -> None:
        # member_dirs=None models a cargo metadata failure; a path dependency must be
        # refused, never accepted, when membership cannot be determined.
        msg = check_isolation.classify_dependency(
            "ade-alpha", {"path": "../alpha"}, self.beta_manifest, self.root, None
        )
        self.assertIsNotNone(msg)
        self.assertIn("fail-closed", msg)


class WorkspaceMembershipViaCargoMetadataTests(unittest.TestCase):
    """End-to-end membership through `cargo metadata`, proving `workspace.exclude` is
    honoured (a `members` glob match is not membership) and that a resolution failure fails
    closed. Skipped only where cargo is genuinely unavailable."""

    def setUp(self) -> None:
        if shutil.which("cargo") is None:
            self.skipTest("cargo is not available in this environment")
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)

    def tearDown(self) -> None:
        if hasattr(self, "_tmp"):
            self._tmp.cleanup()

    def _workspace(self, members: str, crates: list[str], exclude: str = "") -> None:
        ws = f'[workspace]\nresolver = "2"\nmembers = {members}\n'
        if exclude:
            ws += f"exclude = {exclude}\n"
        (self.root / "Cargo.toml").write_text(ws, encoding="utf-8")
        for crate_name in crates:
            crate = self.root / "crates" / crate_name
            (crate / "src").mkdir(parents=True, exist_ok=True)
            (crate / "Cargo.toml").write_text(
                f'[package]\nname = "ade-{crate_name}"\nversion = "0.0.0"\n'
                'edition = "2021"\n',
                encoding="utf-8",
            )
            (crate / "src" / "lib.rs").write_text("", encoding="utf-8")

    def _dir(self, crate_name: str) -> Path:
        return (self.root / "crates" / crate_name).resolve()

    def test_excluded_crate_is_not_a_member_even_though_glob_matches(self) -> None:
        self._workspace(
            '["crates/*"]', ["alpha", "beta", "experimental"],
            exclude='["crates/experimental"]',
        )
        members = check_isolation.resolve_workspace_member_dirs(self.root)
        self.assertIsNotNone(members)
        self.assertIn(self._dir("alpha"), members)
        self.assertNotIn(self._dir("experimental"), members)
        beta_manifest = self.root / "crates" / "beta" / "Cargo.toml"
        # A dependency on the EXCLUDED crate (which matches the members glob) is rejected...
        self.assertIsNotNone(
            check_isolation.classify_dependency(
                "ade-experimental", {"path": "../experimental"}, beta_manifest, self.root,
                members,
            )
        )
        # ...while a dependency on a genuine member passes.
        self.assertIsNone(
            check_isolation.classify_dependency(
                "ade-alpha", {"path": "../alpha"}, beta_manifest, self.root, members
            )
        )

    def test_only_explicitly_listed_members_are_members(self) -> None:
        self._workspace('["crates/alpha"]', ["alpha", "gamma"])
        members = check_isolation.resolve_workspace_member_dirs(self.root)
        self.assertIsNotNone(members)
        self.assertIn(self._dir("alpha"), members)
        self.assertNotIn(self._dir("gamma"), members)

    def test_correct_glob_member_passes_when_not_excluded(self) -> None:
        self._workspace('["crates/*"]', ["alpha", "beta"])
        members = check_isolation.resolve_workspace_member_dirs(self.root)
        self.assertIsNotNone(members)
        self.assertIn(self._dir("alpha"), members)
        self.assertIn(self._dir("beta"), members)

    def test_metadata_failure_returns_none_fail_closed(self) -> None:
        # A directory with no Cargo.toml makes cargo metadata fail -> None (fail closed),
        # never a silent empty set.
        empty = self.root / "empty"
        empty.mkdir()
        self.assertIsNone(check_isolation.resolve_workspace_member_dirs(empty))


if __name__ == "__main__":
    unittest.main()
