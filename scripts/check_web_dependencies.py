#!/usr/bin/env python3
"""Fail-closed Web dependency and repository policy gate.

This gate is deliberately separate from the Rust dependency policy in ADR-0009. It admits
no Web dependency by implication: the direct package, exact version, dependency class,
package manager, scripts, lockfile digest and every locked source must all satisfy the
audited contract in ``policy/web-dependencies.json``.

The current repository is in a contract-only state. A future Web source integration must
add ``web/package.json`` and ``web/package-lock.json`` together and replace the null
approved lock digest with the SHA-256 of the reviewed lockfile. Until then the absence of
both files is valid and any partial or unaudited integration fails closed.
"""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parent.parent
POLICY_PATH = Path("policy/web-dependencies.json")
EXACT_SEMVER = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?$")
HEX_SHA256 = re.compile(r"^[0-9a-f]{64}$")
IGNORED_SCAN_DIRS = {".git", "target", "node_modules", ".npm", "__pycache__"}
ALTERNATE_MANAGER_FILES = {
    ".npmrc",
    "bun.lock",
    "bun.lockb",
    "deno.lock",
    "npm-shrinkwrap.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "yarn.lock",
}
DEPENDENCY_TABLES = {
    "dependencies": "production",
    "devDependencies": "development",
}
PROHIBITED_DEPENDENCY_TABLES = {
    "bundledDependencies",
    "bundleDependencies",
    "optionalDependencies",
    "peerDependencies",
}
LIFECYCLE_SCRIPTS = {
    "preinstall",
    "install",
    "postinstall",
    "prepare",
    "prepublish",
    "prepublishOnly",
    "prepack",
    "postpack",
}


class GateError(ValueError):
    """A deterministic policy violation."""


def _reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise GateError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(path: Path) -> dict:
    try:
        data = json.loads(
            path.read_text(encoding="utf-8"), object_pairs_hook=_reject_duplicate_keys
        )
    except OSError as exc:
        raise GateError(f"cannot read {path}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise GateError(f"malformed JSON in {path}: {exc}") from exc
    if not isinstance(data, dict):
        raise GateError(f"{path} must contain a JSON object")
    return data


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _require_dict(value: object, label: str) -> dict:
    if not isinstance(value, dict):
        raise GateError(f"{label} must be an object")
    return value


def _require_list(value: object, label: str) -> list:
    if not isinstance(value, list):
        raise GateError(f"{label} must be an array")
    return value


def load_policy(root: Path) -> dict:
    policy = read_json(root / POLICY_PATH)
    if policy.get("schema_version") != 1:
        raise GateError("unsupported Web dependency policy schema")
    if policy.get("policy_state") != "contract-only":
        raise GateError("Web policy_state must remain 'contract-only' until source integration")
    web_root = policy.get("web_root")
    if web_root != "web" or Path(web_root).is_absolute() or ".." in Path(web_root).parts:
        raise GateError("policy web_root must be the repository-local 'web' directory")
    if policy.get("manifest_name") != "@autonomous-drone-expert/web":
        raise GateError("unexpected Web package identity")

    manager = _require_dict(policy.get("package_manager"), "package_manager")
    if manager != {"name": "npm", "version": "11.13.0", "lockfile_version": 3}:
        raise GateError("package manager contract must be exactly npm@11.13.0 / lockfile v3")

    direct = _require_dict(policy.get("direct_dependencies"), "direct_dependencies")
    if set(direct) != {"production", "development"}:
        raise GateError("direct_dependencies must contain production and development only")
    seen: set[str] = set()
    for dependency_class in ("production", "development"):
        table = _require_dict(direct[dependency_class], dependency_class)
        for name, record_value in table.items():
            if name in seen:
                raise GateError(f"dependency appears in two classes: {name}")
            seen.add(name)
            record = _require_dict(record_value, f"policy entry {name}")
            version = record.get("version")
            if not isinstance(version, str) or not EXACT_SEMVER.fullmatch(version):
                raise GateError(f"policy version for {name} is not exact semver")
            if not isinstance(record.get("license"), str) or not record.get("license"):
                raise GateError(f"policy license missing for {name}")
            if not isinstance(record.get("reason"), str) or not record.get("reason"):
                raise GateError(f"policy reason missing for {name}")

    approved_digest = policy.get("approved_lockfile_sha256")
    if approved_digest is not None and (
        not isinstance(approved_digest, str) or not HEX_SHA256.fullmatch(approved_digest)
    ):
        raise GateError("approved_lockfile_sha256 must be null or lowercase SHA-256")

    _require_dict(policy.get("allowed_root_scripts"), "allowed_root_scripts")
    _require_list(policy.get("allowed_registry_hosts"), "allowed_registry_hosts")
    _require_list(policy.get("allowed_licenses"), "allowed_licenses")
    _require_list(policy.get("allowed_install_scripts"), "allowed_install_scripts")
    _require_list(
        policy.get("allowed_optional_android_prebuilts"),
        "allowed_optional_android_prebuilts",
    )
    _require_list(policy.get("forbidden_android_tokens"), "forbidden_android_tokens")
    return policy


def repository_files(root: Path) -> list[Path]:
    if (root / ".git").exists():
        try:
            result = subprocess.run(
                ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
                cwd=root,
                capture_output=True,
                check=True,
            )
        except (OSError, subprocess.CalledProcessError) as exc:
            raise GateError(f"cannot enumerate repository files with git: {exc}") from exc
        candidates = [
            root / Path(item.decode("utf-8"))
            for item in result.stdout.split(b"\0")
            if item
        ]
        return [path for path in candidates if path.is_file() or path.is_symlink()]

    files: list[Path] = []
    for path in root.rglob("*"):
        rel = path.relative_to(root)
        if any(part in IGNORED_SCAN_DIRS for part in rel.parts):
            continue
        if path.is_file():
            files.append(path)
    return files


def check_repository_layout(root: Path, policy: dict) -> None:
    web_root = root / policy["web_root"]
    allowed_packages = {web_root / "package.json", web_root / "package-lock.json"}
    for path in repository_files(root):
        if path.name in ALTERNATE_MANAGER_FILES:
            raise GateError(f"unapproved package-manager artifact: {path.relative_to(root)}")
        if path.name in {"package.json", "package-lock.json"} and path not in allowed_packages:
            raise GateError(
                f"package-manager artifact outside canonical web/: {path.relative_to(root)}"
            )


def _android_like(name: str, tokens: list[str]) -> bool:
    lowered = name.lower()
    for token in tokens:
        candidate = token.lower()
        if candidate == "android" and candidate in lowered:
            return True
        if candidate.endswith("/") and lowered.startswith(candidate):
            return True
        if lowered == candidate or lowered.startswith(f"{candidate}-"):
            return True
    return False


def _allowlist_records(policy: dict, key: str) -> dict[tuple[str, str], dict]:
    result: dict[tuple[str, str], dict] = {}
    for value in policy[key]:
        record = _require_dict(value, key)
        name = record.get("name")
        version = record.get("version")
        if not isinstance(name, str) or not isinstance(version, str):
            raise GateError(f"{key} records require string name and version")
        identity = (name, version)
        if identity in result:
            raise GateError(f"duplicate {key} record: {name}@{version}")
        result[identity] = record
    return result


def validate_manifest(manifest: dict, policy: dict) -> None:
    if manifest.get("name") != policy["manifest_name"]:
        raise GateError("web/package.json has the wrong package name")
    if manifest.get("private") is not True:
        raise GateError("web/package.json must set private=true")
    manager = policy["package_manager"]
    if manifest.get("packageManager") != f"{manager['name']}@{manager['version']}":
        raise GateError("web/package.json packageManager does not match the pinned policy")
    if manifest.get("workspaces") not in (None, []):
        raise GateError("nested JavaScript workspaces are prohibited")
    for table_name in PROHIBITED_DEPENDENCY_TABLES:
        if manifest.get(table_name) not in (None, {}, []):
            raise GateError(f"{table_name} is prohibited")

    forbidden_tokens = policy["forbidden_android_tokens"]
    direct_policy = policy["direct_dependencies"]
    for table_name, dependency_class in DEPENDENCY_TABLES.items():
        actual = _require_dict(manifest.get(table_name, {}), table_name)
        expected = direct_policy[dependency_class]
        if set(actual) != set(expected):
            missing = sorted(set(expected) - set(actual))
            extra = sorted(set(actual) - set(expected))
            raise GateError(
                f"{table_name} differs from allowlist (missing={missing}, unapproved={extra})"
            )
        for name, spec in actual.items():
            if _android_like(name, forbidden_tokens):
                raise GateError(f"Android/mobile direct dependency is prohibited: {name}")
            if not isinstance(spec, str) or not EXACT_SEMVER.fullmatch(spec):
                raise GateError(f"dependency {name} must use an exact semver, not {spec!r}")
            expected_version = expected[name]["version"]
            if spec != expected_version:
                raise GateError(
                    f"dependency version drift for {name}: {spec} != {expected_version}"
                )

    scripts = _require_dict(manifest.get("scripts", {}), "scripts")
    for name in scripts:
        if name in LIFECYCLE_SCRIPTS or any(
            name == f"pre{allowed}" or name == f"post{allowed}"
            for allowed in policy["allowed_root_scripts"]
        ):
            raise GateError(f"lifecycle/pre/post script is prohibited: {name}")
    if scripts != policy["allowed_root_scripts"]:
        raise GateError("root scripts must exactly match the audited command allowlist")


def _package_name_from_lock_path(lock_path: str) -> str:
    if "node_modules/" not in lock_path:
        raise GateError(f"lock package path is not under node_modules/: {lock_path}")
    name = lock_path.rsplit("node_modules/", 1)[1]
    if not name or "/node_modules/" in name:
        raise GateError(f"malformed lock package path: {lock_path}")
    return name


def _record_matches_requirements(entry: dict, record: dict) -> bool:
    if record.get("require_dev") is True and entry.get("dev") is not True:
        return False
    if record.get("require_optional") is True and entry.get("optional") is not True:
        return False
    return True


def validate_lockfile(lock: dict, lock_path: Path, manifest: dict, policy: dict) -> None:
    if lock.get("lockfileVersion") != policy["package_manager"]["lockfile_version"]:
        raise GateError("package-lock.json must use the audited lockfile version")
    if lock.get("requires") is not True:
        raise GateError("package-lock.json must set requires=true")
    packages = _require_dict(lock.get("packages"), "package-lock packages")
    root_entry = _require_dict(packages.get(""), "package-lock root package")
    for table_name in DEPENDENCY_TABLES:
        if root_entry.get(table_name, {}) != manifest.get(table_name, {}):
            raise GateError(f"package-lock root {table_name} does not equal package.json")
    if root_entry.get("name") != manifest.get("name"):
        raise GateError("package-lock root package name does not equal package.json")

    approved_digest = policy["approved_lockfile_sha256"]
    if approved_digest is None:
        raise GateError("Web source integration has no approved lockfile SHA-256")
    actual_digest = sha256(lock_path)
    if actual_digest != approved_digest:
        raise GateError(
            f"package-lock digest drift: {actual_digest} != approved {approved_digest}"
        )

    allowed_hosts = set(policy["allowed_registry_hosts"])
    allowed_licenses = set(policy["allowed_licenses"])
    install_scripts = _allowlist_records(policy, "allowed_install_scripts")
    android_prebuilts = _allowlist_records(policy, "allowed_optional_android_prebuilts")
    android_tokens = policy["forbidden_android_tokens"]

    for package_path, value in packages.items():
        if package_path == "":
            continue
        entry = _require_dict(value, f"lock entry {package_path}")
        name = _package_name_from_lock_path(package_path)
        version = entry.get("version")
        if not isinstance(version, str) or not EXACT_SEMVER.fullmatch(version):
            raise GateError(f"locked package {name} has non-exact version {version!r}")
        if entry.get("link") is True or "resolved" not in entry:
            raise GateError(f"locked package {name} is local/linked or has no registry source")
        resolved = entry.get("resolved")
        if not isinstance(resolved, str):
            raise GateError(f"locked package {name} has an invalid source")
        parsed = urlparse(resolved)
        if parsed.scheme != "https" or parsed.hostname not in allowed_hosts:
            raise GateError(f"locked package {name} uses an unapproved source: {resolved}")
        integrity = entry.get("integrity")
        if not isinstance(integrity, str) or not integrity.startswith("sha512-"):
            raise GateError(f"locked package {name} lacks sha512 integrity")
        if entry.get("license") not in allowed_licenses:
            raise GateError(f"locked package {name} has an unapproved/missing license")

        identity = (name, version)
        if entry.get("hasInstallScript") is True:
            record = install_scripts.get(identity)
            if record is None or not _record_matches_requirements(entry, record):
                raise GateError(f"unapproved install script: {name}@{version}")
        if _android_like(name, android_tokens):
            record = android_prebuilts.get(identity)
            if record is None or not _record_matches_requirements(entry, record):
                raise GateError(f"Android/mobile transitive package is prohibited: {name}@{version}")

    for table_name in DEPENDENCY_TABLES:
        for name, version in manifest.get(table_name, {}).items():
            entry = packages.get(f"node_modules/{name}")
            if not isinstance(entry, dict) or entry.get("version") != version:
                raise GateError(f"direct dependency missing/drifted in lock: {name}@{version}")


def check_repository(root: Path = ROOT) -> None:
    root = root.resolve()
    policy = load_policy(root)
    check_repository_layout(root, policy)
    web_root = root / policy["web_root"]
    manifest_path = web_root / "package.json"
    lock_path = web_root / "package-lock.json"
    if web_root.is_symlink() or manifest_path.is_symlink() or lock_path.is_symlink():
        raise GateError("web package paths must not be symbolic links")
    if manifest_path.exists() != lock_path.exists():
        raise GateError("web/package.json and web/package-lock.json must be added together")
    if not manifest_path.exists():
        if policy["approved_lockfile_sha256"] is not None:
            raise GateError("approved lock digest exists but the Web package is absent")
        return
    manifest = read_json(manifest_path)
    validate_manifest(manifest, policy)
    lock = read_json(lock_path)
    validate_lockfile(lock, lock_path, manifest, policy)


def main() -> int:
    try:
        check_repository()
    except GateError as exc:
        print("WEB DEPENDENCY POLICY GATE FAILED")
        print(f"  - {exc}")
        return 1
    print("web dependency policy gate passed (audited contract-only state)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
