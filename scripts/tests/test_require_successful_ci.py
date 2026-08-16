from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from require_successful_ci import select_latest_run  # noqa: E402


def run(number: int, *, sha: str = "a" * 40, conclusion: str = "success") -> dict:
    return {
        "id": number,
        "run_number": number,
        "run_attempt": 1,
        "head_sha": sha,
        "name": "CI",
        "path": ".github/workflows/ci.yml",
        "event": "pull_request",
        "status": "completed",
        "conclusion": conclusion,
        "html_url": f"https://github.example/runs/{number}",
    }


class CanonicalCiSelectionTests(unittest.TestCase):
    def test_latest_exact_success_is_selected(self) -> None:
        self.assertEqual(select_latest_run([run(1), run(2)], "a" * 40)["id"], 2)

    def test_latest_failure_overrides_older_success(self) -> None:
        with self.assertRaisesRegex(ValueError, "not successful"):
            select_latest_run([run(1), run(2, conclusion="failure")], "a" * 40)

    def test_wrong_sha_name_path_and_event_are_rejected(self) -> None:
        probes = [
            run(1, sha="b" * 40),
            {**run(2), "name": "Not CI"},
            {**run(3), "path": ".github/workflows/not-ci.yml"},
            {**run(4), "event": "pull_request_target"},
        ]
        with self.assertRaisesRegex(ValueError, "no canonical CI"):
            select_latest_run(probes, "a" * 40)


if __name__ == "__main__":
    unittest.main()
