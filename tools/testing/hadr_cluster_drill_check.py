#!/usr/bin/env python3
"""Simulation-only HA/DR cluster drill readiness checker."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Sequence


DEFAULT_ROOT = Path(__file__).resolve().parents[2]

EVIDENCE_DOCUMENTS = (
    Path("docs/runbooks/hadr.md"),
    Path("docs/runbooks/README.md"),
    Path("docs/testing/release-gates.md"),
)

REQUIRED_PATHS = (
    "docs/runbooks/hadr.md",
    "docs/runbooks/README.md",
    "docs/testing/release-gates.md",
    "crates/andromeda-storage/tests/hadr_promotion_runtime_contract.rs",
    "crates/andromeda-storage/tests/hadr_membership_store_contract.rs",
    "crates/andromeda-storage/tests/quorum_membership_contract.rs",
    "crates/andromeda-storage/tests/wal_shipping_reclaimability_contract.rs",
    "crates/andromeda-storage/tests/promotion_boundary_contract.rs",
    "crates/andromeda-quic/tests/hadr_stream_mapping_contract.rs",
    "crates/andromeda-audit/tests/hadr_backup_audit_contract.rs",
    "crates/andromeda-storage/src/hadr",
    "crates/andromeda-cli/src/hadr",
    "crates/andromeda-quic/src/hadr_streams.rs",
)

EXPECTED_COMMANDS = (
    "cargo test -p andromeda-storage --test hadr_promotion_runtime_contract --locked -- --nocapture",
    "cargo test -p andromeda-storage --test hadr_membership_store_contract --locked -- --nocapture",
    "cargo test -p andromeda-storage --test quorum_membership_contract --locked -- --nocapture",
    "cargo test -p andromeda-storage --test wal_shipping_reclaimability_contract --locked -- --nocapture",
    "cargo test -p andromeda-quic --test hadr_stream_mapping_contract --locked -- --nocapture",
    "cargo test -p andromeda-audit --test hadr_backup_audit_contract --locked -- --nocapture",
)

EXPECTED_DOC_TOKENS = (
    (
        "docs/runbooks/hadr.md",
        "HA/DR controls are available only through Administration or HA/DR surfaces.",
    ),
    (
        "docs/runbooks/hadr.md",
        "cargo test -p andromeda-audit --test hadr_backup_audit_contract",
    ),
    (
        "docs/runbooks/README.md",
        "Planned or unplanned HA/DR failover",
    ),
    (
        "docs/testing/release-gates.md",
        "Backup/PITR/HA/DR",
    ),
)

LIMITATIONS = (
    "This script is simulation-only and does not start nodes, open sockets, publish cluster manifests, or fence a real primary.",
    "Passing this script is local readiness evidence only; it is not release proof for production HA/DR readiness.",
    "Release proof still requires a recorded cluster simulation with primary crash, partition, quorum, fencing proof, candidate recovery, promotion, manifest update, and replica repointing.",
)


@dataclass(frozen=True)
class Gap:
    category: str
    path: str
    message: str


@dataclass(frozen=True)
class PathResult:
    path: str
    exists: bool


@dataclass(frozen=True)
class CommandResult:
    command: str
    found_in: tuple[str, ...]
    target_paths: tuple[str, ...]
    gaps: tuple[Gap, ...]


@dataclass(frozen=True)
class TokenResult:
    path: str
    token: str
    found: bool


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Simulation-only Andromeda HA/DR cluster drill checker. "
            "The script verifies local artifacts and documented evidence commands only."
        ),
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=DEFAULT_ROOT,
        help="Repository root. Defaults to the root inferred from this script.",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit with status 1 when required local evidence is missing.",
    )
    parser.add_argument(
        "--format",
        choices=("text", "json"),
        default="text",
        help="Output format. The script never writes report files.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Shortcut for --format json.",
    )
    parser.add_argument(
        "--github-annotations",
        action="store_true",
        help="Emit GitHub Actions annotations for missing local evidence.",
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


def normalize_command(command: str) -> str:
    return re.sub(r"\s+", " ", command).strip()


def cargo_package(command: str) -> str | None:
    match = re.search(r"(?:^|\s)-p\s+([A-Za-z0-9_-]+)", command)
    return match.group(1) if match else None


def cargo_test_target(command: str) -> str | None:
    match = re.search(r"(?:^|\s)--test\s+([A-Za-z0-9_-]+)", command)
    return match.group(1) if match else None


def command_document_hits(root: Path, command: str) -> tuple[str, ...]:
    normalized_command = normalize_command(command)
    hits: list[str] = []
    for document in EVIDENCE_DOCUMENTS:
        path = root / document
        text = read_text(path)
        normalized_text = normalize_command(text)
        if normalized_command in normalized_text:
            hits.append(rel(root, path))
    return tuple(hits)


def command_target_paths(root: Path, command: str) -> tuple[str, ...]:
    package = cargo_package(command)
    if package is None:
        return ()

    target = cargo_test_target(command)
    crate_dir = root / "crates" / package
    if target is None:
        return (rel(root, crate_dir / "tests"),)

    return (rel(root, crate_dir / "tests" / f"{target}.rs"),)


def command_gaps(root: Path, command: str) -> tuple[Gap, ...]:
    gaps: list[Gap] = []
    package = cargo_package(command)
    if package is None:
        gaps.append(
            Gap(
                "command",
                ".",
                f"Cannot identify cargo package in evidence command: {command}",
            )
        )
        return tuple(gaps)

    crate_dir = root / "crates" / package
    manifest = crate_dir / "Cargo.toml"
    if not manifest.exists():
        gaps.append(
            Gap(
                "command",
                rel(root, manifest),
                f"Evidence command references missing crate package {package}.",
            )
        )
        return tuple(gaps)

    target = cargo_test_target(command)
    if target is None:
        tests_dir = crate_dir / "tests"
        if not tests_dir.exists() or not any(tests_dir.glob("*.rs")):
            gaps.append(
                Gap(
                    "command",
                    rel(root, tests_dir),
                    "Evidence command has no --test target and the crate has no integration tests.",
                )
            )
        return tuple(gaps)

    target_path = crate_dir / "tests" / f"{target}.rs"
    target_dir = crate_dir / "tests" / target
    if not target_path.exists() and not target_dir.exists():
        gaps.append(
            Gap(
                "command",
                rel(root, target_path),
                f"Evidence command references missing integration test target {target}.",
            )
        )

    return tuple(gaps)


def build_path_results(root: Path) -> tuple[PathResult, ...]:
    return tuple(
        PathResult(path=path, exists=(root / path).exists())
        for path in REQUIRED_PATHS
    )


def build_command_results(root: Path) -> tuple[CommandResult, ...]:
    results: list[CommandResult] = []
    for command in EXPECTED_COMMANDS:
        hits = command_document_hits(root, command)
        gaps = list(command_gaps(root, command))
        if not hits:
            gaps.append(
                Gap(
                    "command",
                    ".",
                    f"Evidence command is not documented in the expected runbooks or validation matrix: {command}",
                )
            )
        results.append(
            CommandResult(
                command=command,
                found_in=hits,
                target_paths=command_target_paths(root, command),
                gaps=tuple(gaps),
            )
        )
    return tuple(results)


def build_token_results(root: Path) -> tuple[TokenResult, ...]:
    results: list[TokenResult] = []
    for path_text, token in EXPECTED_DOC_TOKENS:
        text = read_text(root / path_text)
        results.append(TokenResult(path=path_text, token=token, found=token in text))
    return tuple(results)


def build_report(root: Path) -> dict[str, object]:
    root = root.resolve()
    path_results = build_path_results(root)
    command_results = build_command_results(root)
    token_results = build_token_results(root)

    gaps: list[Gap] = []
    for result in path_results:
        if not result.exists:
            gaps.append(
                Gap(
                    "path",
                    result.path,
                    "Required HA/DR cluster drill artifact is missing.",
                )
            )
    for result in command_results:
        gaps.extend(result.gaps)
    for result in token_results:
        if not result.found:
            gaps.append(
                Gap(
                    "documentation",
                    result.path,
                    f"Required documentation token is missing: {result.token}",
                )
            )

    deduped_gaps = tuple(
        {
            (gap.category, gap.path, gap.message): gap
            for gap in gaps
        }.values()
    )

    return {
        "schema": "andromeda.hadr_cluster_drill_check.v1",
        "root": str(root),
        "mode": "simulation-only",
        "status": "FAIL" if deduped_gaps else "PASS",
        "path_results": [asdict(result) for result in path_results],
        "command_results": [asdict(result) for result in command_results],
        "token_results": [asdict(result) for result in token_results],
        "gaps": [asdict(gap) for gap in deduped_gaps],
        "limitations": LIMITATIONS,
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
            f"title={annotation_escape(item['category'])}::"
            f"{annotation_escape(item['message'])}"
        )


def print_text_report(report: dict[str, object]) -> None:
    print("Andromeda HA/DR Cluster Drill Check")
    print(f"Repository: {report['root']}")
    print(f"Mode: {report['mode']}")
    print(f"Status: {report['status']}")
    print()

    path_results = report["path_results"]
    assert isinstance(path_results, list)
    found_paths = sum(1 for item in path_results if item["exists"])
    print(f"Required Artifacts: {found_paths} found, {len(path_results) - found_paths} missing")
    for item in path_results:
        assert isinstance(item, dict)
        status = "found" if item["exists"] else "missing"
        print(f"  {status}: {item['path']}")
    print()

    print("Evidence Commands")
    for item in report["command_results"]:
        assert isinstance(item, dict)
        found_in = item["found_in"]
        gaps = item["gaps"]
        assert isinstance(found_in, tuple | list)
        assert isinstance(gaps, tuple | list)
        print(
            f"  {item['command']}: {len(found_in)} document hits, {len(gaps)} gaps"
        )
        for document in found_in:
            print(f"    documented in: {document}")
        for gap in gaps:
            print(f"    gap: {gap['path']} - {gap['message']}")
    print()

    gaps = report["gaps"]
    assert isinstance(gaps, list)
    print("Gaps")
    if not gaps:
        print("  none")
    for item in gaps:
        assert isinstance(item, dict)
        print(f"  [{item['category']}] {item['path']} - {item['message']}")
    print()

    print("Limitations")
    for limitation in report["limitations"]:
        print(f"  {limitation}")
    print()
    print("Result: HA/DR cluster drill check completed.")


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    report = build_report(args.root)

    if args.github_annotations:
        emit_github_annotations(report)

    output_format = "json" if args.json else args.format
    if output_format == "json":
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print_text_report(report)

    return 1 if args.strict and report["status"] == "FAIL" else 0


if __name__ == "__main__":
    sys.exit(main())
