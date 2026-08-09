#!/usr/bin/env python3
"""Isolation, dependency and publication gates.

These gates enforce the independence rules in ADR-0001 and the temporary licensing
posture in ADR-0004. They target *real coupling* -- imports, paths, submodules, remotes,
vendored copies -- and deliberately do not fail merely because another project is named
in documentation.

Standard library only. No production dependency is introduced by this script.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parent.parent

# Dependency policy (ADR-0009, narrowed by ADR-0012). First-party path dependencies onto
# actual members of this workspace remain the default and all historical checks remain in
# force. ADR-0012 admits exactly two audited crates, in exactly one manifest and dependency
# class each. The declaration shape is part of the allowlist: aliases, feature drift, version
# drift, table drift, or moving either dependency to another manifest fails closed.
#
# "Actual member" is decided by Cargo itself via `cargo metadata` (below), so that
# `workspace.exclude` is honoured -- a `workspace.members` glob match is NOT membership.

ALLOWED_REMOTES = {"origin"}

AUDITED_REGISTRY_DEPENDENCIES = {
    (
        PurePosixPath("crates/web-storage-wasm-bridge/Cargo.toml"),
        "dependencies",
        "wasm-bindgen",
    ): {
        "version": "=0.2.127",
        "default-features": False,
        "features": ["std"],
    },
    (
        PurePosixPath("tools/wasm-bindgen-cli-support/Cargo.toml"),
        "dependencies",
        "wasm-bindgen-cli-support",
    ): {"version": "=0.2.127"},
}

LICENSE_FILENAMES = {
    "license",
    "license.md",
    "license.txt",
    "licence",
    "licence.md",
    "licence.txt",
    "copying",
    "copying.md",
    "copying.txt",
}

VENDORED_DIRS = {"vendor", "third_party", "thirdparty", "node_modules", "external"}

PUBLICATION_MARKERS = (
    "cargo publish",
    "npm publish",
    "pnpm publish",
    "yarn publish",
    "gh release",
    "action-gh-release",
    "create-release",
    "crates.io/api",
)

errors: list[str] = []


def fail(message: str) -> None:
    errors.append(message)


def tracked_files() -> list[Path]:
    out = subprocess.run(
        ["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True
    )
    return [ROOT / line for line in out.stdout.splitlines() if line]


def check_no_submodules(files: list[Path]) -> None:
    if (ROOT / ".gitmodules").exists():
        fail("A .gitmodules file exists. Submodules are prohibited (ADR-0001).")
    for path in files:
        if path.name == ".gitmodules":
            fail(f"Submodule configuration tracked at {path.relative_to(ROOT)}.")


def check_no_license_file(files: list[Path]) -> None:
    for path in files:
        if path.name.lower() in LICENSE_FILENAMES:
            fail(
                f"{path.relative_to(ROOT)} exists. The final license is deferred and no "
                "LICENSE file may be added before that decision (ADR-0004)."
            )


def check_no_vendored_copies(files: list[Path]) -> None:
    for path in files:
        parts = {part.lower() for part in path.relative_to(ROOT).parts[:-1]}
        hit = parts & VENDORED_DIRS
        if hit:
            fail(
                f"{path.relative_to(ROOT)} sits under a vendored directory "
                f"({', '.join(sorted(hit))}). Copying another project into this "
                "repository is prohibited (ADR-0001)."
            )


def check_no_publication_workflow() -> None:
    workflows = ROOT / ".github" / "workflows"
    if not workflows.is_dir():
        return
    for path in sorted(workflows.glob("*.y*ml")):
        text = path.read_text(encoding="utf-8")
        lowered = text.lower()
        for marker in PUBLICATION_MARKERS:
            if marker in lowered:
                fail(
                    f"{path.relative_to(ROOT)} contains publication marker '{marker}'. "
                    "Publishing any artefact is prohibited until the license decision "
                    "is made (ADR-0004)."
                )
        if re.search(r"^\s{0,4}release:\s*$", text, re.MULTILINE):
            fail(
                f"{path.relative_to(ROOT)} declares a 'release:' trigger. "
                "Release workflows are prohibited in this stage."
            )


def path_escapes_repository(manifest_path: Path, dep_path: str, root: Path = ROOT) -> bool:
    """Return True when a path dependency resolves outside the repository.

    A string prefix comparison is not sufficient here: `/x/repo-malicious` starts with
    `/x/repo`, so a sibling directory with a similar name would slip through. Path
    containment is checked structurally instead.
    """
    resolved = (manifest_path.parent / dep_path).resolve()
    return not resolved.is_relative_to(root.resolve())


def _iter_dependency_tables(manifest: dict):
    for key in ("dependencies", "dev-dependencies", "build-dependencies"):
        if key in manifest:
            yield key, manifest[key]
    for target in manifest.get("target", {}).values():
        for key in ("dependencies", "dev-dependencies", "build-dependencies"):
            if key in target:
                yield f"target.{key}", target[key]


def resolve_workspace_member_dirs(root: Path = ROOT) -> set[Path] | None:
    """The resolved directories of the ACTUAL workspace members, per Cargo.

    Uses `cargo metadata --no-deps --offline` so that both `workspace.members` **and**
    `workspace.exclude` are applied exactly as Cargo resolves them -- a glob match alone is
    not membership. `--no-deps --offline` performs no network access and lists only the
    workspace's own member packages.

    Returns None on ANY failure (cargo missing, non-zero exit, unparseable output, a package
    without a manifest path). Callers MUST treat None as **fail-closed** -- refuse the path
    dependency -- never as "the workspace has no members".
    """
    try:
        result = subprocess.run(
            [
                "cargo",
                "metadata",
                "--no-deps",
                "--format-version",
                "1",
                "--offline",
                "--manifest-path",
                str(root / "Cargo.toml"),
            ],
            cwd=root,
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError:
        return None
    if result.returncode != 0:
        return None
    try:
        data = json.loads(result.stdout)
    except json.JSONDecodeError:
        return None
    packages = data.get("packages")
    if not isinstance(packages, list):
        return None
    dirs: set[Path] = set()
    for package in packages:
        manifest_path = package.get("manifest_path")
        if not isinstance(manifest_path, str):
            return None
        dirs.add(Path(manifest_path).resolve().parent)
    return dirs


def is_workspace_member(resolved_dir: Path, member_dirs: set[Path]) -> bool:
    """True when `resolved_dir` is one of the actual member directories (cargo metadata)."""
    return resolved_dir.resolve() in member_dirs


def read_package_name(cargo_toml: Path) -> str | None:
    """The `[package].name` of a manifest, or None if unreadable/malformed/absent."""
    try:
        manifest = tomllib.loads(cargo_toml.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError):
        return None
    package = manifest.get("package")
    if not isinstance(package, dict):
        return None
    name = package.get("name")
    return name if isinstance(name, str) else None


def classify_dependency(
    dep_name: str,
    spec,
    manifest_path: Path,
    root: Path = ROOT,
    member_dirs: set[Path] | None = None,
    table_name: str = "dependencies",
) -> str | None:
    """Return an error message if `spec` violates the policy, else None.

    The normal accepted form is a path dependency onto a crate that is an ACTUAL member of
    this workspace (per `cargo metadata`, so `workspace.exclude` is honoured), whose real
    package name matches the declaration (allowing an explicit ``package = "…"`` rename).
    ADR-0012 additionally permits the exact declarations in
    ``AUDITED_REGISTRY_DEPENDENCIES``. Registry versions, git sources, wildcards, hybrids,
    workspace-inherited deps, escaping paths and non-member paths are otherwise rejected.
    Path containment is checked structurally (resolve + relative_to), never by string prefix.

    `member_dirs` is the set of resolved member directories from
    [`resolve_workspace_member_dirs`]; None means membership could not be determined, and any
    path dependency is then refused **fail-closed**.
    """
    relative_manifest = PurePosixPath(manifest_path.relative_to(root).as_posix())
    prefix = f"{relative_manifest}: dependency '{dep_name}'"

    expected_registry_spec = AUDITED_REGISTRY_DEPENDENCIES.get(
        (relative_manifest, table_name, dep_name)
    )

    def classify_registry_spec() -> str | None:
        if expected_registry_spec is None:
            return (
                f"{prefix} is an unaudited registry dependency. Only the exact "
                "manifest/table/name/spec shapes in ADR-0012 are permitted."
            )
        if spec != expected_registry_spec:
            return (
                f"{prefix} drifts from its audited ADR-0012 declaration "
                f"(expected {expected_registry_spec!r}, found {spec!r})."
            )
        return None

    if isinstance(spec, str):
        if spec.strip() == "*":
            return f"{prefix} uses a wildcard version. Prohibited (ADR-0009)."
        return classify_registry_spec()
    if not isinstance(spec, dict):
        return f"{prefix} has an unrecognised specification. Prohibited (ADR-0009)."

    if "git" in spec:
        return f"{prefix} uses a git source. Prohibited (ADR-0009)."
    if "registry" in spec or "registry-index" in spec:
        return f"{prefix} names a registry. Prohibited (ADR-0009)."

    dep_path = spec.get("path")
    if not isinstance(dep_path, str):
        if spec.get("workspace") is True:
            return (
                f"{prefix} is workspace-inherited with no local path; it cannot be "
                "resolved to a pinned local member. Prohibited (ADR-0009)."
            )
        return classify_registry_spec()
    if dep_path.strip() == "*":
        return f"{prefix} uses a wildcard path. Prohibited (ADR-0009)."
    version = spec.get("version")
    if version is not None:
        return (
            f"{prefix} mixes a path with a version ('{version}'). Hybrid path+registry "
            "dependencies are prohibited (ADR-0009)."
        )

    if path_escapes_repository(manifest_path, dep_path, root):
        return f"{prefix} has a path escaping the repository ({dep_path})."

    if member_dirs is None:
        return (
            f"{prefix} is a path dependency but workspace membership could not be "
            "determined (cargo metadata failed). Refusing it, fail-closed (ADR-0009)."
        )
    resolved = (manifest_path.parent / dep_path).resolve()
    if not is_workspace_member(resolved, member_dirs):
        return (
            f"{prefix} path '{dep_path}' is not an actual member of this workspace (check "
            "workspace.members and workspace.exclude). Only workspace members may be "
            "depended upon (ADR-0009)."
        )

    actual = read_package_name(resolved / "Cargo.toml")
    expected = spec.get("package", dep_name)
    if actual is None:
        return f"{prefix} path '{dep_path}' does not resolve to a readable Cargo package."
    if not isinstance(expected, str) or actual != expected:
        return (
            f"{prefix} is an alias hiding a different package (declared '{expected}', "
            f"found '{actual}'). Prohibited (ADR-0009)."
        )
    return None


def check_cargo_manifests(files: list[Path]) -> None:
    manifests: list[tuple[Path, dict]] = []
    any_path_dependency = False
    for path in files:
        if path.name != "Cargo.toml":
            continue
        rel = path.relative_to(ROOT)
        try:
            manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        except tomllib.TOMLDecodeError as exc:
            fail(f"{rel}: malformed Cargo.toml ({exc}).")
            continue
        manifests.append((path, manifest))
        for _table_name, table in _iter_dependency_tables(manifest):
            for _name, spec in (table or {}).items():
                if isinstance(spec, dict) and isinstance(spec.get("path"), str):
                    any_path_dependency = True

    # Determine actual membership from Cargo ONLY when a path dependency exists. With none
    # (the current state), cargo is never invoked and the gate stays dependency-free. When a
    # path dependency does exist and membership cannot be resolved, member_dirs is None and
    # every path dependency is refused fail-closed by classify_dependency.
    member_dirs = resolve_workspace_member_dirs(ROOT) if any_path_dependency else set()

    for path, manifest in manifests:
        for table_name, table in _iter_dependency_tables(manifest):
            for name, spec in (table or {}).items():
                message = classify_dependency(
                    name, spec, path, ROOT, member_dirs, table_name
                )
                if message:
                    fail(message)


def check_remotes() -> None:
    try:
        out = subprocess.run(
            ["git", "remote"], cwd=ROOT, capture_output=True, text=True, check=True
        )
    except (OSError, subprocess.CalledProcessError):
        print("note: git remotes could not be inspected in this environment; skipped.")
        return
    remotes = {line.strip() for line in out.stdout.splitlines() if line.strip()}
    unexpected = remotes - ALLOWED_REMOTES
    if unexpected:
        fail(
            f"Unexpected git remotes: {sorted(unexpected)}. Only 'origin' is allowed "
            "(ADR-0001)."
        )


def main() -> int:
    files = tracked_files()
    check_no_submodules(files)
    check_no_license_file(files)
    check_no_vendored_copies(files)
    check_no_publication_workflow()
    check_cargo_manifests(files)
    check_remotes()

    if errors:
        print("ISOLATION GATE FAILED\n")
        for item in errors:
            print(f"  - {item}")
        return 1
    print(f"isolation gate passed ({len(files)} tracked files inspected)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
