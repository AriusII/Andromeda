#!/usr/bin/env python3
"""Read-only P03 catalog/DefinitionBatch durability checker."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Sequence


DEFAULT_ROOT = Path(__file__).resolve().parents[2]
P03_TASK_DIR = Path(".work/codex/p03-catalog-definitionbatch-durability")

P03_TEST_FILES = (
    "crates/andromeda-catalog/tests/catalog_store_contract.rs",
    "crates/andromeda-catalog/tests/catalog_store_contract/mutation_wal.rs",
    "crates/andromeda-catalog/tests/catalog_store_contract/recovery.rs",
    "crates/andromeda-catalog/tests/catalog_digest_contract.rs",
    "crates/andromeda-definition-batch/tests/alter_drop_compat.rs",
    "crates/andromeda-catalog-recovery/tests/mutation_replay_anomalies.rs",
    "crates/andromeda-catalog-recovery/tests/catalog_publication_subscription_runtime_contract.rs",
    "crates/andromeda-plan-cache/tests/plan_invalidation.rs",
    "crates/andromeda-procedure-contract/tests/procedure_contract_digest.rs",
)

SOURCE_SCAN_DIRS = (
    "crates/andromeda-catalog/src",
    "crates/andromeda-definition-batch/src",
    "crates/andromeda-catalog-recovery/src",
    "crates/andromeda-plan-cache/src",
    "crates/andromeda-procedure-contract/src",
)

EXPECTED_REPORTS = (
    (
        "catalog-store-durable-evidence",
        "docs/roadmap/sources/P03_CATALOG_STORE_DURABLE_EVIDENCE.md",
        ("catalog store", "durable", "lsn", "catalogmutationrecord"),
    ),
    (
        "definition-batch-apply-evidence",
        "docs/roadmap/sources/P03_DEFINITION_BATCH_APPLY_EVIDENCE.md",
        ("definitionbatch", "apply", "transactional", "rollback"),
    ),
    (
        "contract-hash-canonicalization",
        "docs/roadmap/sources/P03_CONTRACT_HASH_CANONICALIZATION.md",
        ("contracthash", "canonical", "observable", "format"),
    ),
    (
        "catalog-recovery-report",
        "docs/roadmap/sources/P03_CATALOG_RECOVERY_REPORT.md",
        ("catalogrecoveryreport", "recovery", "complete", "ordered"),
    ),
)

GLOBAL_INVARIANTS = (
    (
        "CatalogMutationRecord Begin/Apply/Commit",
        ("CatalogMutationRecord", "CatalogChangeBegin", "CatalogChangeApply", "CatalogChangeCommit"),
    ),
    (
        "durable publication evidence before visibility",
        (
            "CatalogMutationCommitEvidence",
            "durable_lsn",
            "flush_through",
            "DurablePublicationExternal",
        ),
    ),
    (
        "dense apply indexes",
        ("operation_index", "DuplicateApplyIndex", "SparseApplyIndexes"),
    ),
    (
        "source and dependency graph hashes",
        ("source_hash", "dependency_graph_hash", "DefinitionBatchDependencyGraphHash"),
    ),
    (
        "DefinitionBatch transactional apply",
        ("apply_definition_batch_durably", "rollback", "all-or-nothing"),
    ),
    (
        "compatibility policy additive/breaking/deprecated/rejected",
        ("AdditiveOnly", "breaking", "deprecated", "rejected"),
    ),
    (
        "PlanCache binding to CatalogVersion and ContractHash",
        ("PlanCacheKey", "CatalogVersion", "ContractHash", "PolicyVersion", "PlanClass"),
    ),
    (
        "CatalogRecoveryReport complete ordered replay",
        ("CatalogRecoveryReport", "EndOfLogBeforeCommit", "ApplyRecordOrderMismatch", "VersionGap"),
    ),
    (
        "CatalogPublicationReport Administration/HA durable evidence",
        ("CatalogPublicationReport", "AdministrationHaOnly", "durable WAL LSN", "durable marker evidence"),
    ),
)

NEGATIVE_TEST_MARKERS = (
    ("incomplete batch", ("EndOfLogBeforeCommit", "incomplete")),
    ("duplicate apply", ("DuplicateApplyIndex", "duplicate")),
    ("out-of-order apply", ("ApplyRecordOrderMismatch", "reordered")),
    ("stale base catalog", ("VersionGap", "stale")),
    ("contract breaking change", ("breaking", "ExactHash")),
)


@dataclass(frozen=True)
class Gap:
    path: str
    category: str
    message: str


@dataclass(frozen=True)
class EvidenceFile:
    path: str
    kind: str


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    parser.add_argument("--strict", action="store_true")
    parser.add_argument("--format", choices=("text", "json"), default="text")
    parser.add_argument("--json", action="store_true", help="Compatibility alias for --format json.")
    return parser.parse_args(argv)


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError:
        return ""


def rel_path(root: Path, path: Path) -> str:
    try:
        return path.relative_to(root).as_posix()
    except ValueError:
        return path.as_posix()


def normalize(text: str) -> str:
    return re.sub(r"\s+", " ", text.lower())


def has_token(text: str, token: str) -> bool:
    return normalize(token) in normalize(text)


def has_all_tokens(text: str, tokens: Sequence[str]) -> bool:
    return all(has_token(text, token) for token in tokens)


def required_test_gaps(root: Path) -> list[Gap]:
    gaps: list[Gap] = []
    for rel in P03_TEST_FILES:
        path = root / rel
        if not path.exists():
            gaps.append(Gap(rel, "missing-test", "Required P03 test file is missing."))
            continue
        if not read_text(path).strip():
            gaps.append(Gap(rel, "empty-test", "Required P03 test file is empty."))
    return gaps


def existing_report_files(root: Path) -> list[Path]:
    reports: list[Path] = []
    source_dir = root / "docs/roadmap/sources"
    if source_dir.exists():
        reports.extend(sorted(source_dir.glob("P03_*.md")))

    task_dir = root / P03_TASK_DIR
    if task_dir.exists():
        for path in sorted(task_dir.rglob("*.md")):
            rel = path.relative_to(task_dir).as_posix()
            if rel.startswith("plans/"):
                continue
            reports.append(path)
    return sorted(set(reports))


def collect_scan_text(root: Path) -> tuple[str, list[EvidenceFile]]:
    chunks: list[str] = []
    files: list[EvidenceFile] = []

    for rel in P03_TEST_FILES:
        path = root / rel
        text = read_text(path)
        if text:
            chunks.append(text)
            files.append(EvidenceFile(rel, "test"))

    for rel_dir in SOURCE_SCAN_DIRS:
        base = root / rel_dir
        if not base.exists():
            continue
        for path in sorted(base.rglob("*.rs")):
            text = read_text(path)
            if text:
                chunks.append(text)
                files.append(EvidenceFile(rel_path(root, path), "source"))

    for path in existing_report_files(root):
        text = read_text(path)
        if text:
            chunks.append(text)
            files.append(EvidenceFile(rel_path(root, path), "report"))

    return "\n".join(chunks), files


def invariant_gaps(scan_text: str) -> list[Gap]:
    gaps: list[Gap] = []
    for label, tokens in GLOBAL_INVARIANTS:
        if not has_all_tokens(scan_text, tokens):
            gaps.append(Gap("<P03 evidence corpus>", "invariant", f"Missing P03 invariant `{label}`."))

    for label, tokens in NEGATIVE_TEST_MARKERS:
        if not has_all_tokens(scan_text, tokens):
            gaps.append(Gap("<P03 evidence corpus>", "negative-test", f"Missing P03 negative test marker `{label}`."))
    return gaps


def report_gaps(root: Path) -> tuple[list[Gap], list[dict[str, object]]]:
    gaps: list[Gap] = []
    summaries: list[dict[str, object]] = []
    for report_id, rel, tokens in EXPECTED_REPORTS:
        path = root / rel
        text = read_text(path)
        exists = path.exists()
        token_gaps = [token for token in tokens if not has_token(text, token)]
        summaries.append(
            {
                "id": report_id,
                "path": rel,
                "present": exists,
                "token_gaps": token_gaps,
                "satisfied": exists and not token_gaps,
            }
        )
        if not exists:
            gaps.append(Gap(rel, "missing-report", f"Missing P03 report `{report_id}`."))
            continue
        for token in token_gaps:
            gaps.append(Gap(rel, "report-token", f"Report `{report_id}` is missing `{token}`."))
    return gaps, summaries


def build_report(root: Path) -> dict[str, object]:
    root = root.resolve()
    gaps = required_test_gaps(root)
    scan_text, scanned_files = collect_scan_text(root)
    gaps.extend(invariant_gaps(scan_text))
    report_gap_list, report_summaries = report_gaps(root)
    gaps.extend(report_gap_list)

    return {
        "schema": "andromeda.p03_catalog_definition_batch_durability.v1",
        "root": str(root),
        "status": "FAIL" if gaps else "PASS",
        "task_dir": P03_TASK_DIR.as_posix(),
        "required_tests": list(P03_TEST_FILES),
        "scanned_files": [asdict(file) for file in scanned_files],
        "required_reports": report_summaries,
        "gaps": [asdict(gap) for gap in gaps],
    }


def print_text(report: dict[str, object]) -> None:
    print("Andromeda P03 Catalog DefinitionBatch Durability")
    print(f"Repository: {report['root']}")
    print(f"Status: {report['status']}")
    print()

    required_reports = report["required_reports"]
    assert isinstance(required_reports, list)
    satisfied = sum(1 for item in required_reports if isinstance(item, dict) and item["satisfied"])
    print(f"Required reports: {satisfied}/{len(required_reports)}")
    for item in required_reports:
        assert isinstance(item, dict)
        marker = "present" if item["satisfied"] else "missing"
        print(f"  [{marker}] {item['id']}")

    gaps = report["gaps"]
    assert isinstance(gaps, list)
    print()
    if not gaps:
        print("Gaps: none")
        return
    print("Gaps:")
    for gap in gaps:
        assert isinstance(gap, dict)
        print(f"  [{gap['category']}] {gap['path']}: {gap['message']}")


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    output_format = "json" if args.json else args.format
    report = build_report(args.root)
    if output_format == "json":
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print_text(report)
    return 1 if args.strict and report["status"] == "FAIL" else 0


if __name__ == "__main__":
    sys.exit(main())
