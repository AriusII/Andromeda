#!/usr/bin/env python3
"""Read-only P00 repository state and governance checker."""

from __future__ import annotations

import argparse
import json
import sys
import tomllib
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Sequence


DEFAULT_ROOT = Path(__file__).resolve().parents[2]

EXPECTED_CRATE_COUNT = 90
EXPECTED_RUST_VERSION = "1.95.0"
EXPECTED_EDITION = "2024"
EXPECTED_RESOLVER = "3"

REQUIRED_P00_FILES = (
    "docs/status.md",
    "docs/roadmap/00_CURRENT_STATE_CROSS_CHECK.md",
    "docs/roadmap/sources/CROSS_CHECK_SOURCES.md",
    "docs/project/RUST_BASELINE_1_95.md",
    "docs/project/CRITICALITY_MODEL.md",
    "docs/project/FEATURE_ACCEPTANCE_GATE.md",
    "docs/project/CRATE_CLUSTER_CRITICALITY_MATRIX.md",
    "docs/adr/ADR-0001-RUST_BASELINE_AND_MSRV.md",
    "docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md",
    "docs/adr/ADR-0004-CANONICAL_BINARY_FORMAT.md",
    "docs/adr/ADR-0007-QUIC_RPC_BOUNDARY_NO_GRPC.md",
    "docs/adr/ADR-0017-NO_DYNAMIC_SQL_APPLICATION_SURFACE.md",
)

EXPECTED_CRATE_COUNT_TOKEN = f"{EXPECTED_CRATE_COUNT} crates"
EXPECTED_MATRIX_COUNT_TOKEN = f"{EXPECTED_CRATE_COUNT} workspace crates"

REQUIRED_TOKENS_BY_FILE = {
    "docs/status.md": (
        f"Workspace members: {EXPECTED_CRATE_COUNT_TOKEN}",
        "Rust version: 1.95.0",
        "Edition: 2024",
        "Resolver: 3",
        "Readiness boundary",
    ),
    "docs/roadmap/00_CURRENT_STATE_CROSS_CHECK.md": (
        EXPECTED_CRATE_COUNT_TOKEN,
        "rust-version = \"1.95.0\"",
        "resolver = \"3\"",
        "état actuel",
        "readiness",
    ),
    "docs/roadmap/sources/CROSS_CHECK_SOURCES.md": (
        "Cargo.toml",
        EXPECTED_CRATE_COUNT_TOKEN,
        "Rust 1.95.0",
        "source de vérité",
    ),
    "docs/project/CRITICALITY_MODEL.md": (
        "| C5 |",
        "| C0 |",
        "Evidence required",
        "must not control a higher-criticality invariant",
    ),
    "docs/project/FEATURE_ACCEPTANCE_GATE.md": (
        "existing code",
        "targeted tests",
        "trace",
        "recovery behavior",
    ),
    "docs/project/CRATE_CLUSTER_CRITICALITY_MATRIX.md": (
        EXPECTED_MATRIX_COUNT_TOKEN,
        "P00 governance baseline",
        "`andromeda-wal`",
        "`andromeda-inventory-demo`",
    ),
    "docs/adr/ADR-0001-RUST_BASELINE_AND_MSRV.md": (
        "Rust 1.95.0",
        "Rust 2024 Edition",
        "workspace resolver 3",
    ),
    "docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md": (
        "Cargo workspace",
        "crates aligned to engine responsibilities",
    ),
    "docs/adr/ADR-0004-CANONICAL_BINARY_FORMAT.md": (
        "native layout",
        "canonical binary",
    ),
    "docs/adr/ADR-0007-QUIC_RPC_BOUNDARY_NO_GRPC.md": (
        "QUIC",
        "gRPC",
    ),
    "docs/adr/ADR-0017-NO_DYNAMIC_SQL_APPLICATION_SURFACE.md": (
        "SQL",
        "Procedure",
    ),
}

STALE_PATH_REFERENCES = ("docs/roadmap/ROADMAP.md",)
STALE_CRATE_COUNT_REFERENCE = "88 crates"

READINESS_CLAIM_PATTERNS = (
    "production-ready",
    "production ready",
    "ready for deployment",
)

READINESS_NEGATIONS = (
    "not production-ready",
    "not production ready",
    "not release-ready",
    "must not",
    "may not",
    "do not claim",
    "does not make",
    "no document",
    "no production readiness",
    "not a production",
    "never production-ready",
    "production-readiness guardrail",
    "without retained",
    "readiness remains blocked",
    "blocked status",
    "tant que",
    "refuse",
)


@dataclass(frozen=True)
class Gap:
    path: str
    category: str
    message: str


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    parser.add_argument("--strict", action="store_true")
    parser.add_argument("--json", action="store_true")
    return parser.parse_args(argv)


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError:
        return ""


def load_workspace(root: Path) -> tuple[dict[str, object], list[Gap]]:
    cargo_path = root / "Cargo.toml"
    try:
        return tomllib.loads(read_text(cargo_path)), []
    except tomllib.TOMLDecodeError as err:
        return {}, [Gap("Cargo.toml", "toml", f"Cargo.toml is not valid TOML: {err}")]


def workspace_gaps(root: Path) -> tuple[list[str], list[Gap]]:
    cargo, gaps = load_workspace(root)
    workspace = cargo.get("workspace", {})
    package = workspace.get("package", {}) if isinstance(workspace, dict) else {}
    members = workspace.get("members", []) if isinstance(workspace, dict) else []

    if not isinstance(members, list):
        return [], [*gaps, Gap("Cargo.toml", "workspace", "workspace.members is not a list.")]

    crate_members = [str(member) for member in members if str(member).startswith("crates/")]

    checks = (
        ("workspace.resolver", workspace.get("resolver"), EXPECTED_RESOLVER),
        ("workspace.package.rust-version", package.get("rust-version"), EXPECTED_RUST_VERSION),
        ("workspace.package.edition", package.get("edition"), EXPECTED_EDITION),
    )
    for label, actual, expected in checks:
        if actual != expected:
            gaps.append(Gap("Cargo.toml", "baseline", f"{label} is `{actual}`, expected `{expected}`."))

    if len(crate_members) != EXPECTED_CRATE_COUNT:
        gaps.append(
            Gap(
                "Cargo.toml",
                "crate-count",
                f"workspace declares {len(crate_members)} crate members, expected {EXPECTED_CRATE_COUNT}.",
            )
        )

    return crate_members, gaps


def required_file_gaps(root: Path) -> list[Gap]:
    gaps: list[Gap] = []
    for rel in REQUIRED_P00_FILES:
        path = root / rel
        text = read_text(path)
        if not path.exists():
            gaps.append(Gap(rel, "missing-file", "Required P00 deliverable is missing."))
            continue

        for token in REQUIRED_TOKENS_BY_FILE.get(rel, ()):
            if token.lower() not in text.lower():
                gaps.append(Gap(rel, "token", f"Missing required P00 token `{token}`."))

    return gaps


def stale_reference_gaps(root: Path) -> list[Gap]:
    gaps: list[Gap] = []
    paths = [
        root / "README.md",
        root / "docs/status.md",
        root / "docs/roadmap/00_CURRENT_STATE_CROSS_CHECK.md",
        root / "docs/roadmap/sources/CROSS_CHECK_SOURCES.md",
        root / "docs/roadmap/sources/VALIDATION_REPORT.md",
    ]
    historical_markers = (
        "older",
        "historical",
        "supersedes",
        "ancien",
        "anciens",
        "mentionnait",
        "mentionnaient",
        "référenc",
        "recré",
        "corrige",
        "replaced",
        "souvenir documentaire",
    )
    for path in paths:
        text = read_text(path)
        rel = path.relative_to(root).as_posix()
        for token in STALE_PATH_REFERENCES:
            if token.lower() in text.lower():
                gaps.append(Gap(rel, "stale-reference", f"Contains stale reference `{token}`."))
        for line_number, line in enumerate(text.splitlines(), start=1):
            lowered = line.lower()
            if STALE_CRATE_COUNT_REFERENCE not in lowered:
                continue
            if any(marker in lowered for marker in historical_markers):
                continue
            gaps.append(
                Gap(
                    f"{rel}:{line_number}",
                    "stale-reference",
                    "Contains unbounded `88 crates` reference outside the historical divergence registry.",
                )
            )
    return gaps


def is_negated_readiness_claim(line: str) -> bool:
    lowered = line.lower()
    return any(negation in lowered for negation in READINESS_NEGATIONS)


def readiness_claim_gaps(root: Path) -> list[Gap]:
    gaps: list[Gap] = []
    for base in (root / "docs", root / "crates"):
        for path in sorted(base.rglob("*.md")):
            rel = path.relative_to(root).as_posix()
            lines = read_text(path).splitlines()
            for line_number, line in enumerate(lines, start=1):
                lowered = line.lower()
                if not any(pattern in lowered for pattern in READINESS_CLAIM_PATTERNS):
                    continue
                window = " ".join(lines[max(0, line_number - 3) : min(len(lines), line_number + 2)])
                if is_negated_readiness_claim(window):
                    continue
                gaps.append(
                    Gap(
                        f"{rel}:{line_number}",
                        "readiness-claim",
                        "Production or deployment readiness claim must be bounded by retained evidence.",
                    )
                )
    return gaps


def matrix_gaps(root: Path, crate_members: Sequence[str]) -> list[Gap]:
    matrix = read_text(root / "docs/project/CRATE_CLUSTER_CRITICALITY_MATRIX.md")
    gaps: list[Gap] = []
    for member in crate_members:
        crate_name = member.rsplit("/", maxsplit=1)[-1]
        if f"`{crate_name}`" not in matrix:
            gaps.append(
                Gap(
                    "docs/project/CRATE_CLUSTER_CRITICALITY_MATRIX.md",
                    "matrix",
                    f"Workspace crate `{crate_name}` is missing from the cluster/criticality matrix.",
                )
            )
    return gaps


def build_report(root: Path) -> dict[str, object]:
    root = root.resolve()
    crate_members, gaps = workspace_gaps(root)
    gaps.extend(required_file_gaps(root))
    gaps.extend(stale_reference_gaps(root))
    gaps.extend(readiness_claim_gaps(root))
    gaps.extend(matrix_gaps(root, crate_members))

    return {
        "schema": "andromeda.p00_repository_state.v1",
        "root": str(root),
        "status": "FAIL" if gaps else "PASS",
        "crate_count": len(crate_members),
        "expected_crate_count": EXPECTED_CRATE_COUNT,
        "rust_version": EXPECTED_RUST_VERSION,
        "edition": EXPECTED_EDITION,
        "resolver": EXPECTED_RESOLVER,
        "gaps": [asdict(gap) for gap in gaps],
    }


def print_text(report: dict[str, object]) -> None:
    print("Andromeda P00 Repository State")
    print(f"Repository: {report['root']}")
    print(f"Status: {report['status']}")
    print(f"Crates: {report['crate_count']} expected {report['expected_crate_count']}")
    print(f"Rust: {report['rust_version']} / Edition {report['edition']} / resolver {report['resolver']}")
    print()
    gaps = report["gaps"]
    assert isinstance(gaps, list)
    if not gaps:
        print("Gaps: none")
        return
    print("Gaps:")
    for gap in gaps:
        assert isinstance(gap, dict)
        print(f"  [{gap['category']}] {gap['path']}: {gap['message']}")


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    report = build_report(args.root)
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print_text(report)
    return 1 if args.strict and report["status"] == "FAIL" else 0


if __name__ == "__main__":
    sys.exit(main())
