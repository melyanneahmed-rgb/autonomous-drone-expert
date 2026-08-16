#!/usr/bin/env python3
"""Fail-closed policy gate for the isolated Android packaging-validation project."""

from __future__ import annotations

import json
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ANDROID = ROOT / "android"


def fail(message: str) -> None:
    raise SystemExit(f"android validation policy failed: {message}")


def main() -> int:
    policy = json.loads((ANDROID / "dependency-policy.json").read_text(encoding="utf-8"))
    root_build = (ANDROID / "build.gradle").read_text(encoding="utf-8")
    app_build = (ANDROID / "app/build.gradle").read_text(encoding="utf-8")
    settings = (ANDROID / "settings.gradle").read_text(encoding="utf-8")
    manifest = (ANDROID / "app/src/main/AndroidManifest.xml").read_text(encoding="utf-8")
    lock_path = ANDROID / "app/gradle.lockfile"
    verification_path = ANDROID / "gradle/verification-metadata.xml"

    agp = policy["toolchain"]["android_gradle_plugin"]["version"]
    build_tools = policy["toolchain"]["build_tools"]
    direct_name, direct = next(iter(policy["direct_dependencies"].items()))
    if f'version "{agp}"' not in root_build:
        fail("Android Gradle plugin version drift")
    if f'{direct_name}:{direct["version"]}' not in app_build:
        fail("Android Browser Helper version drift")
    if f'buildToolsVersion "{build_tools}"' not in app_build:
        fail("Android Build Tools version drift")
    if not all(token in settings for token in ["google()", "mavenCentral()", "FAIL_ON_PROJECT_REPOS"]):
        fail("repository source policy drift")
    if re.search(r"(?:latest|\d+\.\+|SNAPSHOT)", root_build + app_build, re.IGNORECASE):
        fail("floating Android dependency version")

    if not lock_path.is_file() or not verification_path.is_file():
        fail("dependency lock or SHA-256 verification metadata is missing")
    runtime = {
        line.split("=", 1)[0]
        for line in lock_path.read_text(encoding="utf-8").splitlines()
        if "debugRuntimeClasspath" in line and not line.startswith("empty=")
    }
    expected_runtime = set(policy["runtime_closure"])
    if runtime != expected_runtime:
        fail(f"runtime closure drift: added={sorted(runtime - expected_runtime)}, removed={sorted(expected_runtime - runtime)}")

    verification = ET.parse(verification_path)
    namespace = {"v": "https://schema.gradle.org/dependency-verification"}
    components = {
        f'{item.attrib["group"]}:{item.attrib["name"]}:{item.attrib["version"]}'
        for item in verification.findall("./v:components/v:component", namespace)
    }
    missing_verification = sorted(expected_runtime - components)
    if missing_verification:
        fail(f"runtime components lack verification metadata: {missing_verification}")
    if not verification.findall(".//v:sha256", namespace):
        fail("verification metadata contains no SHA-256 records")
    locked_components = {
        line.split("=", 1)[0]
        for line in lock_path.read_text(encoding="utf-8").splitlines()
        if line and not line.startswith("#") and line != "empty="
    }
    review = policy["closure_review"]
    if len(locked_components) != review["locked_component_versions"]:
        fail("locked Android component count drift")
    if len(components) != review["checksum_verified_component_versions"]:
        fail("checksum-verified Android component count drift")
    if len(runtime) != review["runtime_component_versions"]:
        fail("runtime Android component count drift")
    if review["runtime_license"] != policy["runtime_closure_license"]:
        fail("runtime Android license review drift")

    permission_names = re.findall(r'<uses-permission android:name="([^"]+)"', manifest)
    if permission_names != ["android.permission.INTERNET"]:
        fail(f"unexpected Android permissions: {permission_names}")
    android_source = "\n".join(
        path.read_text(encoding="utf-8")
        for path in sorted((ANDROID / "app/src").rglob("*"))
        if path.is_file()
    )
    for prohibited in ["android.hardware.usb", "UsbManager", "WebView", "MSP_", "WriteApproval"]:
        if prohibited in android_source:
            fail(f"prohibited native authority token: {prohibited}")
    if "com.google.androidbrowserhelper.trusted.LauncherActivity" not in manifest:
        fail("official Trusted Web Activity launcher is missing")
    if "https://melyanneahmed-rgb.github.io/autonomous-drone-expert/" not in android_source:
        fail("validation wrapper URL drift")

    prohibited_files = [
        path.relative_to(ROOT).as_posix()
        for path in ROOT.rglob("*")
        if path.is_file() and path.suffix.lower() in {".jks", ".keystore", ".p12"}
    ]
    if prohibited_files:
        fail(f"signing material is committed: {prohibited_files}")
    if (ANDROID / "gradle/wrapper/gradle-wrapper.jar").exists():
        fail("opaque Gradle wrapper binary is prohibited")

    print(f"android validation policy passed ({len(runtime)} locked runtime modules)")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, ET.ParseError) as error:
        print(f"android validation policy failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
