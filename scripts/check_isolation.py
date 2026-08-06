#!/usr/bin/env python3
"""Isolation, dependency and publication gates.

These gates enforce the independence rules in ADR-0001 and the temporary licensing
posture in ADR-0004. They target *real coupling* -- imports, paths, submodules, remotes,
vendored copies -- and deliberately do not fail merely because another project is named
in documentation.

Standard library only. No production dependency is introduced by this script.
"""

from __future__ import annotations

import re
import subprocess
import sys
import tomllib
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parent.parent

# Dependency policy (ADR-0009). The foundation batch's absolute "zero dependencies" rule is
# replaced by a precise one: the ONLY dependencies permitted are first-party PATH
# dependencies onto crates that are members of THIS workspace. Everything else -- a registry
# version, a git URL, a wildcard, a path escaping the repository, a path that is not a real
# workspace member, a workspace-inherited dependency, or an alias hiding a different package
# -- is rejected. EXTERNAL PRODUCTION DEPENDENCIES REMAIN PROHIBITED; their supply-chain
# audit is enforced separately by cargo-deny. Relaxing this to admit an external source is a
# new Dependency Audit and its own reviewed pull request, never a quiet edit.
ALLOW_ONLY_WORKSPACE_PATH_DEPENDENCIES = True

ALLOWED_REMOTES = {"origin"}

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


def load_workspace_members(root: Path = ROOT) -> list[str]:
    """The `workspace.members` globs declared by the root manifest (empty on failure)."""
    try:
        manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError):
        return []
    members = manifest.get("workspace", {}).get("members", [])
    return [glob for glob in members if isinstance(glob, str)]


def is_workspace_member(resolved_dir: Path, root: Path, member_globs: list[str]) -> bool:
    """True when `resolved_dir` is a member crate of this workspace holding a package."""
    try:
        rel = resolved_dir.resolve().relative_to(root.resolve())
    except ValueError:
        return False
    rel_posix = PurePosixPath(rel.as_posix())
    if not any(rel_posix.match(glob) for glob in member_globs):
        return False
    return (resolved_dir / "Cargo.toml").is_file()


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
    member_globs: list[str] | None = None,
) -> str | None:
    """Return an error message if `spec` violates the policy, else None.

    The only accepted form is a path dependency onto a crate that is a member of this
    workspace, whose real package name matches the declaration (allowing an explicit
    ``package = "…"`` rename). Registry versions, git sources, wildcards, hybrids,
    workspace-inherited deps, escaping paths and non-member paths are all rejected. Path
    containment is checked structurally (resolve + relative_to), never by string prefix.
    """
    if member_globs is None:
        member_globs = load_workspace_members(root)
    prefix = f"{manifest_path.relative_to(root)}: dependency '{dep_name}'"

    if isinstance(spec, str):
        if spec.strip() == "*":
            return f"{prefix} uses a wildcard version. Prohibited (ADR-0009)."
        return (
            f"{prefix} is a registry/version dependency ('{spec}'). Only first-party "
            "workspace path dependencies are allowed (ADR-0009)."
        )
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
        return (
            f"{prefix} is a registry/version dependency (no path). Only first-party "
            "workspace path dependencies are allowed (ADR-0009)."
        )
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

    resolved = (manifest_path.parent / dep_path).resolve()
    if not is_workspace_member(resolved, root, member_globs):
        return (
            f"{prefix} path '{dep_path}' is not a member crate of this workspace. Only "
            "workspace members may be depended upon (ADR-0009)."
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
    member_globs = load_workspace_members(ROOT)
    for path in files:
        if path.name != "Cargo.toml":
            continue
        rel = path.relative_to(ROOT)
        try:
            manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        except tomllib.TOMLDecodeError as exc:
            fail(f"{rel}: malformed Cargo.toml ({exc}).")
            continue
        for _table_name, table in _iter_dependency_tables(manifest):
            for name, spec in (table or {}).items():
                message = classify_dependency(name, spec, path, ROOT, member_globs)
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
