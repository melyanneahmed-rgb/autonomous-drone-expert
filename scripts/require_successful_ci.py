#!/usr/bin/env python3
"""Prove that canonical CI succeeded for one exact repository commit."""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import Request, urlopen

SHA_PATTERN = re.compile(r"^[0-9a-f]{40}$")


def api_json(repository: str, path: str, token: str) -> dict:
    request = Request(
        f"https://api.github.com/repos/{repository}/{path}",
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "autonomous-drone-expert-ci-provenance",
        },
    )
    with urlopen(request, timeout=30) as response:
        return json.load(response)


def select_latest_run(runs: list[dict], commit_sha: str) -> dict:
    exact = [
        run
        for run in runs
        if run.get("head_sha") == commit_sha
        and run.get("name") == "CI"
        and run.get("path") == ".github/workflows/ci.yml"
        and run.get("event") in {"pull_request", "push", "workflow_dispatch"}
    ]
    if not exact:
        raise ValueError("no canonical CI run exists for the exact commit")
    latest = max(exact, key=lambda run: (run.get("run_number", 0), run.get("run_attempt", 0)))
    if latest.get("status") != "completed" or latest.get("conclusion") != "success":
        raise ValueError(
            f'latest exact canonical CI run is not successful: status={latest.get("status")}, '
            f'conclusion={latest.get("conclusion")}'
        )
    return latest


def write_output(values: dict[str, str]) -> None:
    output_path = os.environ.get("GITHUB_OUTPUT")
    if output_path:
        with Path(output_path).open("a", encoding="utf-8") as handle:
            for key, value in values.items():
                handle.write(f"{key}={value}\n")
    print(json.dumps(values, sort_keys=True))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("commit_sha")
    args = parser.parse_args()

    if not SHA_PATTERN.fullmatch(args.commit_sha):
        raise ValueError("commit SHA must be exactly 40 lowercase hexadecimal characters")
    repository = os.environ.get("GITHUB_REPOSITORY", "")
    token = os.environ.get("GITHUB_TOKEN", "")
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("GITHUB_REPOSITORY is missing or invalid")
    if not token:
        raise ValueError("GITHUB_TOKEN is missing")

    workflow = api_json(
        repository,
        f"actions/workflows/ci.yml/runs?head_sha={quote(args.commit_sha)}&per_page=100",
        token,
    )
    run = select_latest_run(workflow.get("workflow_runs", []), args.commit_sha)
    commit = api_json(repository, f"git/commits/{args.commit_sha}", token)
    tree_sha = commit.get("tree", {}).get("sha", "")
    if not SHA_PATTERN.fullmatch(tree_sha):
        raise ValueError("GitHub did not return a valid tree SHA")

    write_output(
        {
            "commit_sha": args.commit_sha,
            "tree_sha": tree_sha,
            "ci_run_id": str(run["id"]),
            "ci_run_number": str(run["run_number"]),
            "ci_run_attempt": str(run.get("run_attempt", 1)),
            "ci_run_url": run["html_url"],
        }
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, KeyError, HTTPError, URLError, TimeoutError) as error:
        print(f"canonical CI provenance failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
