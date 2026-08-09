"""Adversarial regression tests for the Web dependency policy gate."""

from __future__ import annotations

import copy
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import check_web_dependencies  # noqa: E402


SOURCE_ROOT = Path(__file__).resolve().parents[2]


class WebDependencyGateTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        (self.root / "policy").mkdir()
        (self.root / "web").mkdir()
        self.policy = json.loads(
            (SOURCE_ROOT / "policy" / "web-dependencies.json").read_text(encoding="utf-8")
        )
        self._write_policy()

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _write_json(self, path: Path, value: object) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )

    def _write_policy(self) -> None:
        self._write_json(self.root / "policy" / "web-dependencies.json", self.policy)

    def _manifest(self) -> dict:
        return {
            "name": self.policy["manifest_name"],
            "private": True,
            "packageManager": "npm@11.13.0",
            "scripts": copy.deepcopy(self.policy["allowed_root_scripts"]),
            "dependencies": {
                name: record["version"]
                for name, record in self.policy["direct_dependencies"]["production"].items()
            },
            "devDependencies": {
                name: record["version"]
                for name, record in self.policy["direct_dependencies"]["development"].items()
            },
        }

    def _registry_entry(
        self,
        name: str,
        version: str,
        license_name: str,
        *,
        dev: bool = False,
        optional: bool = False,
    ) -> dict:
        safe_name = name.replace("@", "").replace("/", "-")
        entry = {
            "version": version,
            "resolved": f"https://registry.npmjs.org/{safe_name}/-/{safe_name}-{version}.tgz",
            "integrity": "sha512-Zml4dHVyZS10ZXN0LWludGVncml0eQ==",
            "license": license_name,
        }
        if dev:
            entry["dev"] = True
        if optional:
            entry["optional"] = True
        return entry

    def _lock(self, manifest: dict) -> dict:
        packages: dict[str, dict] = {
            "": {
                "name": manifest["name"],
                "dependencies": copy.deepcopy(manifest["dependencies"]),
                "devDependencies": copy.deepcopy(manifest["devDependencies"]),
            }
        }
        for dependency_class, table_name in (
            ("production", "dependencies"),
            ("development", "devDependencies"),
        ):
            for name, version in manifest[table_name].items():
                record = self.policy["direct_dependencies"][dependency_class][name]
                packages[f"node_modules/{name}"] = self._registry_entry(
                    name,
                    version,
                    record["license"],
                    dev=dependency_class == "development",
                )
        return {
            "name": manifest["name"],
            "lockfileVersion": 3,
            "requires": True,
            "packages": packages,
        }

    def _integrated_fixture(self, lock_mutator=None) -> tuple[dict, dict]:
        self.policy["policy_state"] = "locked"
        manifest = self._manifest()
        lock = self._lock(manifest)
        if lock_mutator is not None:
            lock_mutator(lock)
        self._write_json(self.root / "web" / "package.json", manifest)
        lock_path = self.root / "web" / "package-lock.json"
        self._write_json(lock_path, lock)
        self.policy["approved_lockfile_sha256"] = hashlib.sha256(
            lock_path.read_bytes()
        ).hexdigest()
        self._write_policy()
        return manifest, lock

    def assert_rejected(self, fragment: str) -> None:
        with self.assertRaises(check_web_dependencies.GateError) as caught:
            check_web_dependencies.check_repository(self.root)
        self.assertIn(fragment, str(caught.exception))

    # PASS cases

    def test_contract_only_repository_passes_without_manifest_or_lock(self) -> None:
        self.policy["policy_state"] = "contract-only"
        self.policy["approved_lockfile_sha256"] = None
        self._write_policy()
        check_web_dependencies.check_repository(self.root)

    def test_exact_manifest_and_approved_registry_lock_pass(self) -> None:
        self._integrated_fixture()
        check_web_dependencies.check_repository(self.root)

    def test_audited_optional_install_script_passes_only_with_dev_and_optional(self) -> None:
        def add_fsevents(lock: dict) -> None:
            entry = self._registry_entry("fsevents", "2.3.3", "MIT", dev=True, optional=True)
            entry["hasInstallScript"] = True
            lock["packages"]["node_modules/fsevents"] = entry

        self._integrated_fixture(add_fsevents)
        check_web_dependencies.check_repository(self.root)

    def test_audited_android_named_build_binary_is_not_an_android_product_dep(self) -> None:
        def add_prebuilt(lock: dict) -> None:
            lock["packages"]["node_modules/@rolldown/binding-android-arm64"] = (
                self._registry_entry(
                    "@rolldown/binding-android-arm64",
                    "1.2.3",
                    "MIT",
                    dev=True,
                    optional=True,
                )
            )

        self._integrated_fixture(add_prebuilt)
        check_web_dependencies.check_repository(self.root)

    # Repository and package-manager rejection

    def test_manifest_without_lock_is_rejected(self) -> None:
        self._write_json(self.root / "web" / "package.json", self._manifest())
        self.assert_rejected("must be added together")

    def test_lock_without_manifest_is_rejected(self) -> None:
        self._write_json(self.root / "web" / "package-lock.json", {})
        self.assert_rejected("must be added together")

    def test_alternate_package_manager_is_rejected(self) -> None:
        (self.root / "pnpm-lock.yaml").write_text("lockfileVersion: 9\n", encoding="utf-8")
        self.assert_rejected("unapproved package-manager artifact")

    def test_unreviewed_npm_configuration_is_rejected(self) -> None:
        (self.root / ".npmrc").write_text(
            "registry=https://evil.invalid/\n", encoding="utf-8"
        )
        self.assert_rejected("unapproved package-manager artifact")

    def test_package_manifest_outside_canonical_web_root_is_rejected(self) -> None:
        self._write_json(self.root / "ui" / "package.json", {})
        self.assert_rejected("outside canonical web/")

    def test_package_manifest_cannot_hide_in_generated_named_directory(self) -> None:
        self._write_json(self.root / "build" / "package.json", {})
        self.assert_rejected("outside canonical web/")

    def test_approved_digest_without_web_package_is_rejected(self) -> None:
        self.policy["approved_lockfile_sha256"] = "a" * 64
        self._write_policy()
        self.assert_rejected("locked state requires the Web package and lock")

    def test_contract_only_rejects_web_package_even_with_matching_pair(self) -> None:
        self.policy["policy_state"] = "contract-only"
        self.policy["approved_lockfile_sha256"] = None
        manifest = self._manifest()
        self._write_json(self.root / "web" / "package.json", manifest)
        self._write_json(self.root / "web" / "package-lock.json", self._lock(manifest))
        self._write_policy()
        self.assert_rejected("contract-only state requires")

    def test_contract_only_rejects_non_null_digest(self) -> None:
        self.policy["policy_state"] = "contract-only"
        self.policy["approved_lockfile_sha256"] = "a" * 64
        self._write_policy()
        self.assert_rejected("contract-only state requires a null")

    def test_locked_rejects_null_digest(self) -> None:
        self.policy["policy_state"] = "locked"
        self.policy["approved_lockfile_sha256"] = None
        self._write_policy()
        self.assert_rejected("locked state requires a non-null")

    def test_unknown_policy_state_fails_closed(self) -> None:
        self.policy["policy_state"] = "reviewed-ish"
        self._write_policy()
        self.assert_rejected("unsupported Web policy_state")

    # Manifest rejection

    def test_unapproved_direct_dependency_is_rejected(self) -> None:
        manifest = self._manifest()
        manifest["dependencies"]["axios"] = "1.0.0"
        self._write_json(self.root / "web" / "package.json", manifest)
        self._write_json(self.root / "web" / "package-lock.json", {})
        self.assert_rejected("unapproved=['axios']")

    def test_dependency_class_drift_is_rejected(self) -> None:
        manifest = self._manifest()
        version = manifest["dependencies"].pop("react")
        manifest["devDependencies"]["react"] = version
        self._write_json(self.root / "web" / "package.json", manifest)
        self._write_json(self.root / "web" / "package-lock.json", {})
        self.assert_rejected("differs from allowlist")

    def test_ranges_urls_git_tarballs_local_and_workspace_specs_are_rejected(self) -> None:
        bad_specs = [
            "^19.2.6",
            "~19.2.6",
            "*",
            "latest",
            "git+https://example.invalid/react.git",
            "https://example.invalid/react.tgz",
            "file:../escape",
            "workspace:*",
            "npm:react@19.2.6",
        ]
        for spec in bad_specs:
            with self.subTest(spec=spec):
                manifest = self._manifest()
                manifest["dependencies"]["react"] = spec
                self._write_json(self.root / "web" / "package.json", manifest)
                self._write_json(self.root / "web" / "package-lock.json", {})
                self.assert_rejected("exact semver")

    def test_version_drift_is_rejected(self) -> None:
        manifest = self._manifest()
        manifest["dependencies"]["react"] = "19.2.7"
        self._write_json(self.root / "web" / "package.json", manifest)
        self._write_json(self.root / "web" / "package-lock.json", {})
        self.assert_rejected("version drift")

    def test_android_direct_dependency_is_rejected_even_if_policy_entry_is_injected(self) -> None:
        self.policy["direct_dependencies"]["production"]["@capacitor/core"] = {
            "version": "8.0.0",
            "license": "MIT",
            "reason": "adversarial fixture",
        }
        self._write_policy()
        manifest = self._manifest()
        self._write_json(self.root / "web" / "package.json", manifest)
        self._write_json(self.root / "web" / "package-lock.json", {})
        self.assert_rejected("Android/mobile direct dependency")

    def test_unapproved_root_script_is_rejected(self) -> None:
        manifest = self._manifest()
        manifest["scripts"]["download"] = "curl https://example.invalid/tool"
        self._write_json(self.root / "web" / "package.json", manifest)
        self._write_json(self.root / "web" / "package-lock.json", {})
        self.assert_rejected("exactly match")

    def test_lifecycle_script_is_rejected(self) -> None:
        manifest = self._manifest()
        manifest["scripts"]["postinstall"] = "node setup.js"
        self._write_json(self.root / "web" / "package.json", manifest)
        self._write_json(self.root / "web" / "package-lock.json", {})
        self.assert_rejected("lifecycle/pre/post")

    def test_optional_peer_and_workspace_tables_are_rejected(self) -> None:
        for key, value in (
            ("optionalDependencies", {"x": "1.0.0"}),
            ("peerDependencies", {"x": "1.0.0"}),
            ("workspaces", ["../escape"]),
        ):
            with self.subTest(key=key):
                manifest = self._manifest()
                manifest[key] = value
                self._write_json(self.root / "web" / "package.json", manifest)
                self._write_json(self.root / "web" / "package-lock.json", {})
                expected = "workspaces" if key == "workspaces" else key
                self.assert_rejected(expected)

    # Lockfile rejection

    def test_lock_digest_drift_is_rejected(self) -> None:
        self._integrated_fixture()
        lock_path = self.root / "web" / "package-lock.json"
        lock_path.write_text(lock_path.read_text(encoding="utf-8") + " ", encoding="utf-8")
        self.assert_rejected("digest drift")

    def test_lock_root_manifest_drift_is_rejected(self) -> None:
        def drift(lock: dict) -> None:
            lock["packages"][""]["dependencies"]["react"] = "19.2.5"

        self._integrated_fixture(drift)
        self.assert_rejected("does not equal package.json")

    def test_git_http_or_non_registry_lock_source_is_rejected(self) -> None:
        for source in (
            "git+https://github.com/example/react.git",
            "http://registry.npmjs.org/react/-/react-19.2.6.tgz",
            "https://evil.invalid/react-19.2.6.tgz",
            "file:../escape",
        ):
            with self.subTest(source=source):
                def mutate(lock: dict, source=source) -> None:
                    lock["packages"]["node_modules/react"]["resolved"] = source

                self._integrated_fixture(mutate)
                self.assert_rejected("unapproved source")

    def test_linked_lock_entry_is_rejected(self) -> None:
        def mutate(lock: dict) -> None:
            lock["packages"]["node_modules/react"]["link"] = True

        self._integrated_fixture(mutate)
        self.assert_rejected("local/linked")

    def test_missing_integrity_is_rejected(self) -> None:
        def mutate(lock: dict) -> None:
            del lock["packages"]["node_modules/react"]["integrity"]

        self._integrated_fixture(mutate)
        self.assert_rejected("lacks sha512 integrity")

    def test_missing_or_unapproved_license_is_rejected(self) -> None:
        def mutate(lock: dict) -> None:
            lock["packages"]["node_modules/react"]["license"] = "GPL-3.0-only"

        self._integrated_fixture(mutate)
        self.assert_rejected("unapproved/missing license")

    def test_unapproved_install_script_is_rejected(self) -> None:
        def mutate(lock: dict) -> None:
            lock["packages"]["node_modules/react"]["hasInstallScript"] = True

        self._integrated_fixture(mutate)
        self.assert_rejected("unapproved install script")

    def test_install_script_exception_fails_without_optional_dev_markers(self) -> None:
        def mutate(lock: dict) -> None:
            entry = self._registry_entry("fsevents", "2.3.3", "MIT")
            entry["hasInstallScript"] = True
            lock["packages"]["node_modules/fsevents"] = entry

        self._integrated_fixture(mutate)
        self.assert_rejected("unapproved install script")

    def test_unreviewed_android_transitive_is_rejected(self) -> None:
        def mutate(lock: dict) -> None:
            lock["packages"]["node_modules/react-native"] = self._registry_entry(
                "react-native", "1.0.0", "MIT", dev=True, optional=True
            )

        self._integrated_fixture(mutate)
        self.assert_rejected("Android/mobile transitive package")

    def test_android_prebuilt_exception_requires_exact_optional_dev_identity(self) -> None:
        def mutate(lock: dict) -> None:
            lock["packages"]["node_modules/lightningcss-android-arm64"] = (
                self._registry_entry(
                    "lightningcss-android-arm64", "1.33.0", "MPL-2.0", dev=True
                )
            )

        self._integrated_fixture(mutate)
        self.assert_rejected("Android/mobile transitive package")

    def test_direct_dependency_missing_from_lock_is_rejected(self) -> None:
        def mutate(lock: dict) -> None:
            del lock["packages"]["node_modules/react"]

        self._integrated_fixture(mutate)
        self.assert_rejected("missing/drifted in lock")

    def test_duplicate_json_key_fails_closed(self) -> None:
        policy_path = self.root / "policy" / "web-dependencies.json"
        policy_path.write_text('{"schema_version":1,"schema_version":1}\n', encoding="utf-8")
        self.assert_rejected("duplicate JSON key")


if __name__ == "__main__":
    unittest.main()
