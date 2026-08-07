#!/usr/bin/env python3
"""Fail-closed line and branch coverage gate for M1's critical Rust paths.

Input is the JSON emitted by ``cargo llvm-cov --json``. The gate deliberately ignores the
tool's precomputed percentage and recomputes aggregate totals from integer counters.
Missing files, missing/zero branch counters and malformed values are failures, never 100%.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

CRITICAL_FILES = (
    "crates/casebook/src/lib.rs",
    "crates/core-api/src/lib.rs",
    "crates/execution/src/lib.rs",
    "crates/recovery/src/lib.rs",
    "crates/transport/src/lib.rs",
)
MIN_LINE_PERCENT = 90.0
MIN_BRANCH_PERCENT = 70.0


class CoverageError(ValueError):
    """The report is incomplete, malformed, or below policy."""


def _counters(summary: object, metric: str, filename: str) -> tuple[int, int]:
    if not isinstance(summary, dict) or not isinstance(summary.get(metric), dict):
        raise CoverageError(f"{filename}: missing {metric} counters")
    values = summary[metric]
    count = values.get("count")
    covered = values.get("covered")
    if (
        not isinstance(count, int)
        or isinstance(count, bool)
        or not isinstance(covered, int)
        or isinstance(covered, bool)
        or count < 0
        or covered < 0
        or covered > count
    ):
        raise CoverageError(f"{filename}: invalid {metric} counters")
    return covered, count


def evaluate(report: object) -> tuple[float, float]:
    """Validate and return aggregate ``(line_percent, branch_percent)``."""
    if not isinstance(report, dict) or not isinstance(report.get("data"), list):
        raise CoverageError("report has no data array")
    files: dict[str, dict] = {}
    for data in report["data"]:
        if not isinstance(data, dict) or not isinstance(data.get("files"), list):
            raise CoverageError("report data has no files array")
        for entry in data["files"]:
            if not isinstance(entry, dict) or not isinstance(entry.get("filename"), str):
                raise CoverageError("file entry is malformed")
            normalized = entry["filename"].replace("\\", "/")
            for critical in CRITICAL_FILES:
                if normalized == critical or normalized.endswith(f"/{critical}"):
                    if critical in files:
                        raise CoverageError(f"duplicate critical file: {critical}")
                    files[critical] = entry

    missing = [filename for filename in CRITICAL_FILES if filename not in files]
    if missing:
        raise CoverageError(f"missing critical files: {', '.join(missing)}")

    line_covered = line_count = branch_covered = branch_count = 0
    for filename in CRITICAL_FILES:
        summary = files[filename].get("summary")
        covered, count = _counters(summary, "lines", filename)
        line_covered += covered
        line_count += count
        covered, count = _counters(summary, "branches", filename)
        branch_covered += covered
        branch_count += count

    if line_count == 0:
        raise CoverageError("critical line counter total is zero")
    if branch_count == 0:
        raise CoverageError("critical branch counter total is zero")
    line_percent = 100.0 * line_covered / line_count
    branch_percent = 100.0 * branch_covered / branch_count
    if line_percent < MIN_LINE_PERCENT:
        raise CoverageError(
            f"critical line coverage {line_percent:.2f}% < {MIN_LINE_PERCENT:.2f}%"
        )
    if branch_percent < MIN_BRANCH_PERCENT:
        raise CoverageError(
            f"critical branch coverage {branch_percent:.2f}% < "
            f"{MIN_BRANCH_PERCENT:.2f}%"
        )
    return line_percent, branch_percent


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: check_coverage.py COVERAGE.json", file=sys.stderr)
        return 2
    try:
        report = json.loads(Path(argv[1]).read_text(encoding="utf-8"))
        lines, branches = evaluate(report)
    except (OSError, json.JSONDecodeError, CoverageError) as error:
        print(f"COVERAGE GATE FAILED: {error}", file=sys.stderr)
        return 1
    print(f"COVERAGE GATE PASSED: lines={lines:.2f}% branches={branches:.2f}%")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
