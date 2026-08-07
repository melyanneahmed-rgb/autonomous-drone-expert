"""Regression tests for the fail-closed M1 coverage gate."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import check_coverage  # noqa: E402


def report(lines: tuple[int, int] = (95, 100), branches: tuple[int, int] = (75, 100)):
    files = []
    for filename in check_coverage.CRITICAL_FILES:
        files.append(
            {
                "filename": f"/checkout/{filename}",
                "summary": {
                    "lines": {"covered": lines[0], "count": lines[1]},
                    "branches": {"covered": branches[0], "count": branches[1]},
                },
            }
        )
    return {"data": [{"files": files}]}


class CoverageGateTests(unittest.TestCase):
    def test_complete_report_above_both_thresholds_passes(self) -> None:
        self.assertEqual(check_coverage.evaluate(report()), (95.0, 75.0))

    def test_missing_critical_file_fails(self) -> None:
        data = report()
        data["data"][0]["files"].pop()
        with self.assertRaisesRegex(check_coverage.CoverageError, "missing critical"):
            check_coverage.evaluate(data)

    def test_low_line_coverage_fails_independently(self) -> None:
        with self.assertRaisesRegex(check_coverage.CoverageError, "line coverage"):
            check_coverage.evaluate(report(lines=(89, 100)))

    def test_low_branch_coverage_fails_independently(self) -> None:
        with self.assertRaisesRegex(check_coverage.CoverageError, "branch coverage"):
            check_coverage.evaluate(report(branches=(69, 100)))

    def test_zero_branch_counters_never_become_one_hundred_percent(self) -> None:
        with self.assertRaisesRegex(check_coverage.CoverageError, "branch counter total is zero"):
            check_coverage.evaluate(report(branches=(0, 0)))

    def test_malformed_or_impossible_counters_fail(self) -> None:
        data = report()
        data["data"][0]["files"][0]["summary"]["lines"] = {
            "covered": 101,
            "count": 100,
        }
        with self.assertRaisesRegex(check_coverage.CoverageError, "invalid lines"):
            check_coverage.evaluate(data)


if __name__ == "__main__":
    unittest.main()
