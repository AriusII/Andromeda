#!/usr/bin/env python3
"""Read-only crash/recovery scenario matrix checker for Andromeda."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Sequence


DEFAULT_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MATRIX = Path("docs/testing/CRASH_RECOVERY_TEST_PLAN.md")

EXPECTED_SCENARIOS = (
    "CR-11-WAL",
    "CR-11-STORAGE",
    "CR-11-MANIFEST",
    "CR-11-CATALOG-PUBLICATION",
    "CR-11-MAP-PUBLICATION",
    "CR-11-FORENSIC-STARTUP",
    "CR-11-BACKUP-PITR",
    "CR-11-HADR",
)


@dataclass(frozen=True)
class Scenario:
    scenario_id: str
    scope: str
    failure_point: str
    durable_state: str
    evidence: str
    residual_risk: str


@dataclass(frozen=True)
class Gap:
    scenario_id: str
    path: str
    message: str


@dataclass(frozen=True)
class ScenarioReport:
    scenario_id: str
    evidence_commands: tuple[str, ...]
    gaps: tuple[Gap, ...]


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Read-only Andromeda crash/recovery matrix checker.",
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=DEFAULT_ROOT,
        help="Repository root. Defaults to the root inferred from this script.",
    )
    parser.add_argument(
        "--matrix",
        type=Path,
        default=DEFAULT_MATRIX,
        help="Crash/recovery matrix path relative to the repository root.",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit with status 1 when matrix gaps are found.",
    )
    parser.add_argument(
        "--format",
        choices=("text", "json"),
        default="text",
        help="Output format. The script never writes report files.",
    )
    parser.add_argument(
        "--github-annotations",
        action="store_true",
        help="Emit GitHub Actions annotations for matrix gaps.",
    )
    return parser.parse_args(argv)


def rel(root: Path, path: Path) -> str:
    try:
        return path.relative_to(root).as_posix()
    except ValueError:
        return path.as_posix()


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        return ""


def split_markdown_row(line: str) -> list[str]:
    stripped = line.strip()
    if not stripped.startswith("|") or not stripped.endswith("|"):
        return []
    return [cell.strip() for cell in stripped.strip("|").split("|")]


def normalize_cell(value: str) -> str:
    value = value.replace("<br>", "\n")
    return re.sub(r"\s+", " ", value).strip()


def is_separator_row(cells: Sequence[str]) -> bool:
    return bool(cells) and all(re.fullmatch(r":?-{3,}:?", cell.strip()) for cell in cells)


def extract_crash_table(text: str) -> list[Scenario]:
    lines = text.splitlines()
    start_index: int | None = None
    expected_header = [
        "Scenario",
        "Scope",
        "Crash or failure point",
        "Durable state to prove",
        "Required validation evidence",
        "Residual risk to record",
    ]

    for index, line in enumerate(lines):
        cells = split_markdown_row(line)
        if cells == expected_header:
            start_index = index + 1
            break

    if start_index is None:
        return []

    scenarios: list[Scenario] = []
    for line in lines[start_index:]:
        cells = split_markdown_row(line)
        if not cells:
            break
        if is_separator_row(cells):
            continue
        if len(cells) < 6:
            continue
        scenarios.append(
            Scenario(
                scenario_id=normalize_cell(cells[0]),
                scope=normalize_cell(cells[1]),
                failure_point=normalize_cell(cells[2]),
                durable_state=normalize_cell(cells[3]),
                evidence=normalize_cell(cells[4]),
                residual_risk=normalize_cell(cells[5]),
            )
        )

    return scenarios


def extract_backtick_commands(value: str) -> tuple[str, ...]:
    return tuple(
        command.strip()
        for command in re.findall(r"`([^`]+)`", value)
        if command.strip().startswith("cargo ")
    )


def cargo_package(command: str) -> str | None:
    match = re.search(r"(?:^|\s)-p\s+([A-Za-z0-9_-]+)", command)
    return match.group(1) if match else None


def cargo_test_target(command: str) -> str | None:
    match = re.search(r"(?:^|\s)--test\s+([A-Za-z0-9_-]+)", command)
    return match.group(1) if match else None


def cargo_filter(command: str) -> str | None:
    if "--test" in command or "--tests" in command:
        return None
    match = re.search(r"--locked\s+([A-Za-z0-9_:.-]+)", command)
    if match:
        return match.group(1)
    return None


def file_contains(path: Path, token: str) -> bool:
    if path.suffix not in {".rs", ".toml", ".md"}:
        return False
    return token in read_text(path)


def crate_contains_token(crate_dir: Path, token: str) -> bool:
    for base_name in ("tests", "src"):
        base = crate_dir / base_name
        if not base.exists():
            continue
        for path in base.rglob("*"):
            if path.is_file() and file_contains(path, token):
                return True
    return False


def command_gaps(root: Path, scenario_id: str, command: str) -> list[Gap]:
    package = cargo_package(command)
    if package is None:
        return [
            Gap(
                scenario_id,
                ".",
                f"Cannot identify cargo package in evidence command: {command}",
            )
        ]

    crate_dir = root / "crates" / package
    manifest = crate_dir / "Cargo.toml"
    gaps: list[Gap] = []
    if not manifest.exists():
        gaps.append(
            Gap(
                scenario_id,
                rel(root, manifest),
                f"Evidence command references missing crate package {package}.",
            )
        )
        return gaps

    target = cargo_test_target(command)
    if target is not None:
        candidates = (
            crate_dir / "tests" / f"{target}.rs",
            crate_dir / "tests" / target,
        )
        if not any(path.exists() for path in candidates):
            gaps.append(
                Gap(
                    scenario_id,
                    rel(root, candidates[0]),
                    f"Evidence command references missing integration test target {target}.",
                )
            )
        return gaps

    if " --tests" in command:
        tests_dir = crate_dir / "tests"
        if not tests_dir.exists() or not any(tests_dir.glob("*.rs")):
            gaps.append(
                Gap(
                    scenario_id,
                    rel(root, tests_dir),
                    "Evidence command uses --tests but the crate has no integration test files.",
                )
            )
        return gaps

    token = cargo_filter(command)
    if token is not None and not crate_contains_token(crate_dir, token):
        gaps.append(
            Gap(
                scenario_id,
                rel(root, crate_dir),
                f"Evidence command filter {token} was not found under crate tests or src.",
            )
        )

    return gaps


def scenario_gaps(root: Path, scenario: Scenario) -> tuple[Gap, ...]:
    gaps: list[Gap] = []
    commands = extract_backtick_commands(scenario.evidence)
    evidence_lower = scenario.evidence.lower()

    if not scenario.scope:
        gaps.append(Gap(scenario.scenario_id, ".", "Scenario scope is empty."))
    if not scenario.failure_point:
        gaps.append(Gap(scenario.scenario_id, ".", "Crash or failure point is empty."))
    if not scenario.durable_state:
        gaps.append(Gap(scenario.scenario_id, ".", "Durable state to prove is empty."))
    if not scenario.residual_risk:
        gaps.append(Gap(scenario.scenario_id, ".", "Residual risk is empty."))
    if not commands:
        gaps.append(Gap(scenario.scenario_id, ".", "No cargo evidence commands are recorded."))
    if "no direct owner test is visible" in evidence_lower:
        gaps.append(
            Gap(
                scenario.scenario_id,
                ".",
                "Scenario records that no direct owner test is visible.",
            )
        )

    for command in commands:
        gaps.extend(command_gaps(root, scenario.scenario_id, command))

    return tuple(
        {
            (gap.scenario_id, gap.path, gap.message): gap
            for gap in gaps
        }.values()
    )


def build_report(root: Path, matrix_path: Path) -> dict[str, object]:
    root = root.resolve()
    matrix = matrix_path if matrix_path.is_absolute() else root / matrix_path
    text = read_text(matrix)
    scenarios = extract_crash_table(text)
    by_id = {scenario.scenario_id: scenario for scenario in scenarios}

    reports: list[ScenarioReport] = []
    gaps: list[Gap] = []
    if not matrix.exists():
        gaps.append(
            Gap(
                "MATRIX",
                rel(root, matrix),
                "Crash/recovery matrix document is missing.",
            )
        )
    elif not scenarios:
        gaps.append(
            Gap(
                "MATRIX",
                rel(root, matrix),
                "Crash/Recovery Scenario Matrix table was not found.",
            )
        )

    for scenario_id in EXPECTED_SCENARIOS:
        scenario = by_id.get(scenario_id)
        if scenario is None:
            gap = Gap(
                scenario_id,
                rel(root, matrix),
                "Expected crash/recovery scenario row is missing.",
            )
            gaps.append(gap)
            reports.append(ScenarioReport(scenario_id, (), (gap,)))
            continue

        commands = extract_backtick_commands(scenario.evidence)
        scenario_report_gaps = scenario_gaps(root, scenario)
        gaps.extend(scenario_report_gaps)
        reports.append(ScenarioReport(scenario_id, commands, scenario_report_gaps))

    deduped_gaps = list(
        {
            (gap.scenario_id, gap.path, gap.message): gap
            for gap in gaps
        }.values()
    )

    return {
        "schema": "andromeda.crash_matrix_check.v1",
        "root": str(root),
        "matrix": rel(root, matrix),
        "status": "FAIL" if deduped_gaps else "PASS",
        "expected_scenarios": EXPECTED_SCENARIOS,
        "found_scenarios": tuple(sorted(by_id)),
        "scenario_reports": [asdict(report) for report in reports],
        "gaps": [asdict(gap) for gap in deduped_gaps],
    }


def annotation_escape(value: object) -> str:
    text = str(value)
    return text.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def emit_github_annotations(report: dict[str, object]) -> None:
    for item in report["gaps"]:
        assert isinstance(item, dict)
        print(
            "::error "
            f"file={annotation_escape(item['path'])},"
            f"title={annotation_escape(item['scenario_id'])}::"
            f"{annotation_escape(item['message'])}"
        )


def print_text_report(report: dict[str, object]) -> None:
    print("Andromeda Crash Matrix Check")
    print(f"Repository: {report['root']}")
    print(f"Matrix: {report['matrix']}")
    print(f"Status: {report['status']}")
    print()

    expected = report["expected_scenarios"]
    found = report["found_scenarios"]
    assert isinstance(expected, tuple | list)
    assert isinstance(found, tuple | list)
    print(f"Scenarios: {len(found)} found, {len(expected)} expected")

    for raw_report in report["scenario_reports"]:
        assert isinstance(raw_report, dict)
        commands = raw_report["evidence_commands"]
        gaps = raw_report["gaps"]
        assert isinstance(commands, tuple | list)
        assert isinstance(gaps, tuple | list)
        print(
            f"  {raw_report['scenario_id']}: "
            f"{len(commands)} evidence commands, {len(gaps)} gaps"
        )
    print()

    gaps = report["gaps"]
    assert isinstance(gaps, list)
    print("Crash Matrix Gaps")
    if not gaps:
        print("  none")
    for raw_gap in gaps:
        assert isinstance(raw_gap, dict)
        print(
            f"  [{raw_gap['scenario_id']}] {raw_gap['path']} - {raw_gap['message']}"
        )
    print()
    print("Result: crash matrix check completed.")


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    report = build_report(args.root, args.matrix)

    if args.github_annotations:
        emit_github_annotations(report)

    if args.format == "json":
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print_text_report(report)

    return 1 if args.strict and report["status"] == "FAIL" else 0


if __name__ == "__main__":
    sys.exit(main())
