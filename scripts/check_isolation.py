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
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# The foundation batch is approved with zero production dependencies. Introducing the
# first dependency is its own reviewed pull request that must also enable cargo-deny as a
# required check. Flipping this flag without that pull request is a policy violation.
FOUNDATION_NO_DEPENDENCIES = True

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


def check_cargo_manifests(files: list[Path]) -> None:
    for path in files:
        if path.name != "Cargo.toml":
            continue
        rel = path.relative_to(ROOT)
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        for table_name, table in _iter_dependency_tables(manifest):
            if FOUNDATION_NO_DEPENDENCIES and table:
                fail(
                    f"{rel} declares dependencies in [{table_name}]: "
                    f"{sorted(table)}. The foundation batch is approved with zero "
                    "production dependencies."
                )
            for name, spec in (table or {}).items():
                if not isinstance(spec, dict):
                    continue
                if "git" in spec:
                    fail(f"{rel}: dependency '{name}' uses a git source. Prohibited.")
                dep_path = spec.get("path")
                if isinstance(dep_path, str) and path_escapes_repository(path, dep_path):
                    fail(
                        f"{rel}: dependency '{name}' has a path escaping the "
                        f"repository ({dep_path})."
                    )
        if "workspace" in manifest and "package" in manifest:
            continue


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
