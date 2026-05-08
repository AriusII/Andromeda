#!/usr/bin/env python3
"""Read-only local supply-chain tooling preflight for Andromeda."""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Sequence


ROOT = Path(__file__).resolve().parents[2]
PROBE_TIMEOUT_SECONDS = 10


@dataclass(frozen=True)
class ToolSpec:
    key: str
    label: str
    probe_command: tuple[str, ...]
    path_executable: str | None
    expected_gate: str
    absence_blocks: str


@dataclass(frozen=True)
class ToolResult:
    key: str
    label: str
    available: bool
    path: str | None
    probe_command: tuple[str, ...]
    probe_exit_code: int | None
    version_or_help: str | None
    absence_blocks: str
    expected_gate: str
    note: str


TOOL_SPECS = (
    ToolSpec(
        key="cargo-nextest",
        label="cargo-nextest",
        probe_command=("cargo", "nextest", "--version"),
        path_executable="cargo-nextest",
        expected_gate="cargo nextest run --workspace --all-features",
        absence_blocks=(
            "Blocks the preferred local workspace test gate and any release "
            "evidence that depends on nextest output, profiles, retries, or archives."
        ),
    ),
    ToolSpec(
        key="cargo-audit",
        label="cargo-audit",
        probe_command=("cargo", "audit", "--version"),
        path_executable="cargo-audit",
        expected_gate="cargo audit --deny warnings",
        absence_blocks=(
            "Blocks local RustSec advisory evidence and release review of "
            "unreviewed vulnerability or warning findings."
        ),
    ),
    ToolSpec(
        key="cargo-deny",
        label="cargo-deny",
        probe_command=("cargo", "deny", "--version"),
        path_executable="cargo-deny",
        expected_gate="cargo deny check --all-features",
        absence_blocks=(
            "Blocks local dependency policy evidence for licenses, sources, "
            "banned crates, duplicate versions, and advisory policy checks."
        ),
    ),
    ToolSpec(
        key="cargo-vet",
        label="cargo-vet",
        probe_command=("cargo", "vet", "--version"),
        path_executable="cargo-vet",
        expected_gate="cargo vet",
        absence_blocks=(
            "Blocks local cargo-vet attestation and exemption evidence when "
            "vet governance is configured or required for a release packet."
        ),
    ),
    ToolSpec(
        key="cargo-tree",
        label="cargo tree",
        probe_command=("cargo", "tree", "--help"),
        path_executable=None,
        expected_gate="cargo tree --workspace --locked",
        absence_blocks=(
            "Blocks local transitive dependency graph triage, including owner "
            "paths for advisories, duplicate versions, license concerns, and MSRV drift."
        ),
    ),
)


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Read-only Andromeda supply-chain tooling preflight. The script only "
            "checks local command availability and never installs tools."
        ),
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit machine-readable JSON instead of text.",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit with status 1 when any expected supply-chain tool is missing.",
    )
    return parser.parse_args(argv)


def first_nonempty_line(value: str) -> str | None:
    for line in value.splitlines():
        stripped = line.strip()
        if stripped:
            return stripped
    return None


def display_command(command: Sequence[str]) -> str:
    return " ".join(command)


def run_probe(command: tuple[str, ...]) -> tuple[int | None, str | None, str]:
    if shutil.which(command[0]) is None:
        return None, None, f"{command[0]} is not available on PATH"

    try:
        result = subprocess.run(
            list(command),
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=PROBE_TIMEOUT_SECONDS,
        )
    except subprocess.TimeoutExpired:
        return None, None, f"probe timed out after {PROBE_TIMEOUT_SECONDS} seconds"
    except OSError as exc:
        return None, None, f"probe failed: {exc}"

    summary = first_nonempty_line(result.stdout) or first_nonempty_line(result.stderr)
    if result.returncode == 0:
        return result.returncode, summary, "available"

    note = summary or f"probe exited with status {result.returncode}"
    return result.returncode, summary, note


def check_tool(spec: ToolSpec) -> ToolResult:
    path = shutil.which(spec.path_executable) if spec.path_executable else None
    probe_exit_code, summary, note = run_probe(spec.probe_command)
    available = probe_exit_code == 0
    if available and path is None:
        path = shutil.which(spec.probe_command[0])

    return ToolResult(
        key=spec.key,
        label=spec.label,
        available=available,
        path=path,
        probe_command=spec.probe_command,
        probe_exit_code=probe_exit_code,
        version_or_help=summary,
        absence_blocks=spec.absence_blocks,
        expected_gate=spec.expected_gate,
        note=note,
    )


def check_tools() -> list[ToolResult]:
    return [check_tool(spec) for spec in TOOL_SPECS]


def build_report(results: Sequence[ToolResult], strict: bool) -> dict[str, object]:
    missing = [result for result in results if not result.available]
    return {
        "name": "Andromeda Supply-Chain Tooling Preflight",
        "repository": str(ROOT),
        "mode": "strict" if strict else "report-only",
        "status": "blocked" if missing else "ready",
        "missing_count": len(missing),
        "available_count": len(results) - len(missing),
        "checked_count": len(results),
        "strict_failure": bool(strict and missing),
        "tools": [asdict(result) for result in results],
    }


def print_text(report: dict[str, object]) -> None:
    print(report["name"])
    print(f"Repository: {report['repository']}")
    print(f"Mode: {report['mode']}")
    print(f"Status: {report['status']}")
    print()

    print("Tool Availability")
    for result_data in report["tools"]:
        result = ToolResult(**result_data)
        status = "found" if result.available else "missing"
        path = f" ({result.path})" if result.path else ""
        print(f"  {result.label}: {status}{path}")
        print(f"    probe: {display_command(result.probe_command)}")
        if result.version_or_help:
            print(f"    evidence: {result.version_or_help}")
        if not result.available:
            print(f"    blocks: {result.absence_blocks}")
            print(f"    expected gate: {result.expected_gate}")
            print(f"    note: {result.note}")
    print()

    if report["missing_count"]:
        print("Missing Tool Impact")
        for result_data in report["tools"]:
            result = ToolResult(**result_data)
            if not result.available:
                print(f"  {result.label}: {result.absence_blocks}")
        print()

    print(
        "Result: "
        f"{report['available_count']} available, "
        f"{report['missing_count']} missing, "
        f"{report['checked_count']} checked."
    )


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    results = check_tools()
    report = build_report(results, args.strict)

    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print_text(report)

    return 1 if report["strict_failure"] else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
