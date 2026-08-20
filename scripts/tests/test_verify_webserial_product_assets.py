from __future__ import annotations

import json
import shutil
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

SCRIPTS = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(SCRIPTS))

import verify_webserial_product_assets as assets  # noqa: E402


class WebSerialProductAssetTests(unittest.TestCase):
    def test_repository_assets_match_machine_readable_provenance(self) -> None:
        self.assertEqual(assets.verify(), [])

    def test_built_input_must_be_a_real_wasm_module_when_supplied(self) -> None:
        with TemporaryDirectory() as temporary:
            path = Path(temporary) / "input.wasm"
            path.write_bytes(b"drift")
            self.assertTrue(
                any("not a version-1 WebAssembly module" in error for error in assets.verify(
                    input_wasm=path
                ))
            )

    def test_identical_canonical_regeneration_passes_and_drift_fails(self) -> None:
        with TemporaryDirectory() as temporary:
            generated = Path(temporary)
            for name in assets.EXPECTED_OUTPUT_NAMES:
                shutil.copy2(assets.ROOT / assets.OUTPUT_DIRECTORY / name, generated / name)
            self.assertEqual(assets.verify(generated_dir=generated), [])
            with (generated / "ade_web_readonly_serial_wasm_bridge.js").open(
                "ab"
            ) as handle:
                handle.write(b"\n// drift")
            self.assertTrue(
                any("byte-for-byte regeneration drift" in error for error in assets.verify(
                    generated_dir=generated
                ))
            )

    def test_generated_directory_is_an_exact_two_file_allowlist(self) -> None:
        with TemporaryDirectory() as temporary:
            generated = Path(temporary)
            for name in assets.EXPECTED_OUTPUT_NAMES:
                shutil.copy2(assets.ROOT / assets.OUTPUT_DIRECTORY / name, generated / name)
            (generated / "unexpected.txt").write_text("not allowed", encoding="utf-8")
            self.assertTrue(
                any("unexpected entries" in error for error in assets.verify(generated_dir=generated))
            )

    def test_generator_version_and_output_hash_drift_fail_closed(self) -> None:
        original = json.loads((assets.ROOT / assets.MANIFEST).read_text(encoding="utf-8"))
        for mutation in (
            "version",
            "sha256",
            "name_section",
            "generator_source",
            "input_hash_policy",
            "input_published",
        ):
            with self.subTest(mutation=mutation), TemporaryDirectory() as temporary:
                manifest = json.loads(json.dumps(original))
                if mutation == "version":
                    manifest["generator"]["version"] = "0.2.128"
                elif mutation == "sha256":
                    manifest["outputs"][0]["sha256"] = "0" * 64
                elif mutation == "name_section":
                    manifest["generator"]["remove_name_section"] = False
                elif mutation == "generator_source":
                    manifest["generator"]["isolated_source"]["sha256"] = "0" * 64
                elif mutation == "input_hash_policy":
                    manifest["source"]["input_wasm_hash_policy"] = "cross-host-locked"
                else:
                    manifest["source"]["input_wasm_published"] = True
                path = Path(temporary) / "manifest.json"
                path.write_text(json.dumps(manifest), encoding="utf-8")
                self.assertTrue(assets.verify(manifest_path=path))


if __name__ == "__main__":
    unittest.main()
