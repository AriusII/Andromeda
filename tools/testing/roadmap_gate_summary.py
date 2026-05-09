#!/usr/bin/env python3
"""Read-only roadmap gate evidence summary for Andromeda."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Sequence


DEFAULT_ROOT = Path(__file__).resolve().parents[2]


@dataclass(frozen=True)
class PathCheck:
    category: str
    label: str
    patterns: tuple[str, ...]


@dataclass(frozen=True)
class CheckResult:
    category: str
    label: str
    found: tuple[str, ...]
    missing: tuple[str, ...]


@dataclass(frozen=True)
class Gap:
    category: str
    label: str
    path: str
    message: str


DOC_SPEC_CHECKS = (
    PathCheck(
        "docs/specs",
        "roadmap validation documents",
        (
            "tests/README.md",
            "docs/testing/release-gates.md",
            "docs/testing/release-evidence-template.md",
            "docs/testing/testing-strategy.md",
            "docs/governance/release-gates.md",
            "docs/adr/README.md",
        ),
    ),
    PathCheck(
        "docs/specs",
        "contract and format specifications",
        (
            "docs/specs/README.md",
            "docs/specs/core-contracts.md",
            "docs/specs/storage-wal.md",
            "docs/specs/transaction-recovery.md",
            "docs/specs/rpc-security-audit.md",
            "docs/specs/catalog-srpl.md",
            "docs/specs/hadr-backup-restore.md",
            "docs/specs/advisory-optimizer-hardware.md",
        ),
    ),
)


FUZZ_CHECKS = (
    PathCheck(
        "fuzz",
        "fuzz registry and evidence documents",
        (
            "fuzz/Cargo.toml",
            "tests/fuzzing/targets.toml",
            "fuzz/VALIDATION_MATRIX.md",
            "tests/fuzzing/corpus/manifest.toml",
            "fuzz/generators/generate_seed_corpus.py",
            "fuzz/fuzz_targets/*.rs",
        ),
    ),
)


RUNBOOK_CHECKS = (
    PathCheck(
        "runbooks",
        "operations runbooks",
        (
            "docs/runbooks/README.md",
            "docs/runbooks/backup-restore.md",
            "docs/runbooks/corruption.md",
            "docs/runbooks/replica-lag.md",
            "docs/runbooks/performance.md",
            "docs/runbooks/wal-pressure.md",
        ),
    ),
)


CRITICAL_CRATES = (
    "andromeda-wal",
    "andromeda-storage",
    "andromeda-transaction",
    "andromeda-transaction-log",
    "andromeda-mvcc",
    "andromeda-locking",
    "andromeda-savepoint",
    "andromeda-exec",
    "andromeda-catalog",
    "andromeda-contract",
    "andromeda-core",
    "andromeda-observe",
    "andromeda-proto",
    "andromeda-quic",
    "andromeda-rpc-protocol",
    "andromeda-security-contract",
    "andromeda-srpl",
    "andromeda-srpl-ast",
    "andromeda-srpl-parser",
)


CRATE_CHECKS = (
    PathCheck(
        "crates",
        "mission-critical crate manifests",
        tuple(f"crates/{name}/Cargo.toml" for name in CRITICAL_CRATES),
    ),
)


WORKFLOW_CHECKS = (
    PathCheck(
        "ci",
        "quality and release workflows",
        (
            ".github/workflows/00-ci.yml",
            ".github/workflows/01-rust-matrix.yml",
            ".github/workflows/05-supply-chain.yml",
            ".github/workflows/06-nightly-deep-validation.yml",
            ".github/workflows/07-fuzzing.yml",
            ".github/workflows/16-protocol-doctrine-scan.yml",
            ".github/workflows/release-gate-chain.yml",
            ".github/workflows/perf-regression.yml",
        ),
    ),
)


PATH_CHECKS = (
    *DOC_SPEC_CHECKS,
    *FUZZ_CHECKS,
    *RUNBOOK_CHECKS,
    *CRATE_CHECKS,
    *WORKFLOW_CHECKS,
)


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Read-only Andromeda roadmap gate evidence summary.",
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
        help="Exit with status 1 when roadmap gate gaps are found.",
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
        help="Emit GitHub Actions annotations for missing gate inputs.",
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


def matches(root: Path, pattern: str) -> tuple[Path, ...]:
    if any(char in pattern for char in "*?[]"):
        return tuple(sorted(path for path in root.glob(pattern) if path.exists()))

    path = root / pattern
    return (path,) if path.exists() else ()


def evaluate_path_check(root: Path, check: PathCheck) -> CheckResult:
    found: list[str] = []
    missing: list[str] = []
    for pattern in check.patterns:
        resolved = matches(root, pattern)
        if resolved:
            found.extend(rel(root, path) for path in resolved)
        else:
            missing.append(pattern)
    return CheckResult(
        category=check.category,
        label=check.label,
        found=tuple(dict.fromkeys(found)),
        missing=tuple(missing),
    )


def parse_key_values(raw_line: str) -> tuple[str, str] | None:
    line = raw_line.split("#", 1)[0].strip()
    if not line or "=" not in line:
        return None

    key, value = line.split("=", 1)
    key = key.strip()
    value = value.strip()
    quoted = re.match(r'^"([^"]*)"$', value)
    return key, quoted.group(1) if quoted else value


def parse_fuzz_targets(root: Path) -> list[dict[str, str]]:
    targets_path = root / "tests" / "fuzzing" / "targets.toml"
    targets: list[dict[str, str]] = []
    current: dict[str, str] | None = None

    for raw_line in read_text(targets_path).splitlines():
        line = raw_line.split("#", 1)[0].strip()
        if line == "[[target]]":
            current = {}
            targets.append(current)
            continue
        if line.startswith("[["):
            current = None
            continue
        if current is None:
            continue
        parsed = parse_key_values(raw_line)
        if parsed is not None:
            key, value = parsed
            current[key] = value

    return targets


def fuzz_target_gaps(root: Path) -> list[Gap]:
    targets = parse_fuzz_targets(root)
    if not targets:
        return [
            Gap(
                "fuzz",
                "fuzz target registry",
                "tests/fuzzing/targets.toml",
                "No [[target]] entries were found.",
            )
        ]

    gaps: list[Gap] = []
    for target in targets:
        name = target.get("name", "<unnamed>")
        for key, description, base in (
            ("path", "target source", root / "fuzz"),
            ("corpus_dir", "corpus directory", root),
            ("generator", "seed generator", root),
        ):
            value = target.get(key, "").strip()
            if not value:
                gaps.append(
                    Gap(
                        "fuzz",
                        f"fuzz target {name}",
                        f"tests/fuzzing/targets.toml:{key}",
                        f"Missing {description} entry.",
                    )
                )
                continue

            path = base / value
            if not path.exists():
                gaps.append(
                    Gap(
                        "fuzz",
                        f"fuzz target {name}",
                        rel(root, path),
                        f"Missing {description}.",
                    )
                )

    return gaps


def workspace_member_paths(root: Path) -> set[str]:
    text = read_text(root / "Cargo.toml")
    return {
        match.group(1).replace("\\", "/")
        for match in re.finditer(r'"([^"]+)"', text)
        if match.group(1).replace("\\", "/").startswith("crates/")
    }


def workspace_crate_gaps(root: Path) -> list[Gap]:
    members = workspace_member_paths(root)
    gaps: list[Gap] = []

    for member in sorted(members):
        manifest = root / member / "Cargo.toml"
        if not manifest.exists():
            gaps.append(
                Gap(
                    "crates",
                    "workspace member",
                    rel(root, manifest),
                    "Workspace member is listed in Cargo.toml but its manifest is missing.",
                )
            )

    for name in CRITICAL_CRATES:
        member = f"crates/{name}"
        if member not in members:
            gaps.append(
                Gap(
                    "crates",
                    "critical crate workspace membership",
                    member,
                    "Critical crate is not listed as a root workspace member.",
                )
            )

    return gaps


def result_gaps(results: Sequence[CheckResult]) -> list[Gap]:
    gaps: list[Gap] = []
    for result in results:
        for pattern in result.missing:
            gaps.append(
                Gap(
                    result.category,
                    result.label,
                    pattern,
                    "Required roadmap gate input is missing.",
                )
            )
    return gaps


def build_report(root: Path) -> dict[str, object]:
    root = root.resolve()
    results = [evaluate_path_check(root, check) for check in PATH_CHECKS]
    gaps = [
        *result_gaps(results),
        *fuzz_target_gaps(root),
        *workspace_crate_gaps(root),
    ]
    deduped_gaps = list(
        {
            (gap.category, gap.label, gap.path, gap.message): gap
            for gap in gaps
        }.values()
    )

    return {
        "schema": "andromeda.roadmap_gate_summary.v1",
        "root": str(root),
        "status": "FAIL" if deduped_gaps else "PASS",
        "checks": [asdict(result) for result in results],
        "gaps": [asdict(gap) for gap in deduped_gaps],
    }


def annotation_escape(value: object) -> str:
    text = str(value)
    return text.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def emit_github_annotations(report: dict[str, object]) -> None:
    for item in report["gaps"]:
        assert isinstance(item, dict)
        path = item["path"]
        print(
            "::error "
            f"file={annotation_escape(path)},"
            f"title={annotation_escape(item['category'])}::"
            f"{annotation_escape(item['label'])}: {annotation_escape(item['message'])}"
        )


def print_text_report(report: dict[str, object]) -> None:
    print("Andromeda Roadmap Gate Summary")
    print(f"Repository: {report['root']}")
    print(f"Status: {report['status']}")
    print()

    current_category: str | None = None
    for raw_result in report["checks"]:
        assert isinstance(raw_result, dict)
        category = str(raw_result["category"])
        if category != current_category:
            print(category.title())
            current_category = category

        found = raw_result["found"]
        missing = raw_result["missing"]
        assert isinstance(found, tuple | list)
        assert isinstance(missing, tuple | list)
        print(f"  {raw_result['label']}: {len(found)} found, {len(missing)} missing")
        for path in list(found)[:10]:
            print(f"    found: {path}")
        if len(found) > 10:
            print(f"    found: ... {len(found) - 10} more")
        for path in missing:
            print(f"    missing: {path}")
    print()

    gaps = report["gaps"]
    assert isinstance(gaps, list)
    print("Missing Roadmap Gate Inputs")
    if not gaps:
        print("  none")
    for raw_gap in gaps:
        assert isinstance(raw_gap, dict)
        print(
            f"  [{raw_gap['category']}] {raw_gap['label']}: "
            f"{raw_gap['path']} - {raw_gap['message']}"
        )
    print()
    print("Result: roadmap gate summary completed.")


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    report = build_report(args.root)

    if args.github_annotations:
        emit_github_annotations(report)

    if args.format == "json":
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print_text_report(report)

    return 1 if args.strict and report["status"] == "FAIL" else 0


if __name__ == "__main__":
    sys.exit(main())
