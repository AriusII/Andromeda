#!/usr/bin/env python3
"""Read-only P02 durable Inventory/ProductStock vertical path checker."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Sequence


DEFAULT_ROOT = Path(__file__).resolve().parents[2]
P02_TASK_DIR = Path(".work/codex/p02-durable-vertical-product-stock")

P02_TEST_FILES = (
    "crates/andromeda-inventory-demo/tests/v0_vertical_e2e.rs",
    "crates/andromeda-inventory-demo/tests/v0_vertical_e2e/cataloged_procedure_flow.rs",
    "crates/andromeda-inventory-demo/tests/v0_vertical_e2e/failure_rollback_gates.rs",
    "crates/andromeda-inventory-demo/tests/v0_vertical_e2e/product_stock_path.rs",
    "crates/andromeda-inventory-demo/tests/v0_vertical_e2e/result_stream_metadata.rs",
    "crates/andromeda-inventory-demo/tests/v0_vertical_e2e/support.rs",
    "crates/andromeda-inventory-demo/tests/v0_vertical_e2e/wal_recovery_evidence.rs",
    "crates/andromeda-wal/tests/file_wal_contract.rs",
    "crates/andromeda-recovery/tests/file_wal_recovery_contract.rs",
    "crates/andromeda-storage/tests/product_stock_heap_contract.rs",
    "crates/andromeda-storage-heap/tests/product_stock_heap_contract.rs",
)

SOURCE_SCAN_DIRS = (
    "crates/andromeda-inventory-demo/src",
    "crates/andromeda-wal/src",
    "crates/andromeda-recovery/src",
    "crates/andromeda-storage/src",
    "crates/andromeda-storage-heap/src",
)

EXPECTED_REPORTS = (
    (
        "product-stock-durable-path-acceptance",
        (
            "p02 productstock durable path acceptance",
            "p02_product_stock_durable_path_acceptance",
            "productstock durable path acceptance",
        ),
    ),
    (
        "filewal-recovery-evidence",
        (
            "p02 filewal recovery evidence",
            "p02_filewal_recovery_evidence",
            "filewal recovery evidence",
            "file-wal recovery evidence",
        ),
    ),
    (
        "resultstream-ordering-contract",
        (
            "p02 resultstream ordering contract",
            "p02_result_stream_ordering_contract",
            "resultstream ordering contract",
            "result stream ordering contract",
        ),
    ),
    (
        "pre-transaction-rejection-evidence-matrix",
        (
            "p02 pre-transaction rejection matrix",
            "p02_pre_transaction_rejection_matrix",
            "pre-transaction rejection matrix",
            "pre transaction rejection matrix",
        ),
    ),
)

GLOBAL_INVARIANTS = (
    ("ProductStock", ("productstock", "product stock")),
    ("Inventory.ReserveStock", ("inventory.reservestock",)),
    ("durable_commit_lsn", ("durable_commit_lsn",)),
    ("redo_record_lsn", ("redo_record_lsn",)),
    ("product_stock_commit", ("product_stock_commit",)),
    ("ResultStream metadata/batch/completion", ("resultstream", "metadata", "batch", "completion")),
    ("malformed frame", ("malformed frame", "malformed_execute_frame", "malformed execute frame", "frame-len")),
    ("invalid payload domain", ("invalid payload domain", "invalid_payload_domain", "payload-domain")),
    (
        "transaction-bearing client frame",
        (
            "transaction-bearing client frame",
            "transaction_bearing_execute_frame",
            "transaction-bearing execute frame",
            "frame-txid",
        ),
    ),
    ("contract mismatch", ("contract mismatch", "contract_mismatch")),
    ("missing permission", ("missing permission", "missing_permission")),
    (
        "crash before ack/client ack non-authoritative",
        (
            "crash before ack",
            "crash-before-ack",
            "client ack non-authoritative",
            "client ack/result frames are not durable truth",
            "client ack",
            "non-authoritative",
        ),
    ),
    ("FileWal recovery", ("filewal recovery", "recover_from_file_wal", "file-wal recovery")),
    ("only committed redo", ("only committed redo", "committed_replay_lsns")),
    ("RecoveryReport", ("recoveryreport", "filewalrecoveryreportv0", "report_file_wal_recovery_v0")),
    (
        "reconstructed page rows",
        (
            "reconstructed page rows",
            "decode_product_stock_recovery_rows",
            "decode_product_stock_rows",
            "reconstructs_deterministic_rows",
            "reconstructs typed row",
        ),
    ),
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


def has_any(text: str, alternatives: Sequence[str]) -> bool:
    haystack = normalize(text)
    return any(normalize(alternative) in haystack for alternative in alternatives)


def has_all_terms(text: str, terms: Sequence[str]) -> bool:
    return all(has_any(text, (term,)) for term in terms)


def existing_report_files(root: Path) -> list[Path]:
    task_dir = root / P02_TASK_DIR
    report_files: list[Path] = []
    if task_dir.exists():
        for path in sorted(task_dir.rglob("*.md")):
            rel = path.relative_to(task_dir).as_posix()
            if rel.startswith("plans/"):
                continue
            if rel == "missions/person-02-workspace-gate.md":
                continue
            report_files.append(path)

    source_dir = root / "docs/roadmap/sources"
    if source_dir.exists():
        report_files.extend(sorted(source_dir.glob("P02_*.md")))

    return sorted(set(report_files))


def report_identity_text(root: Path, path: Path, text: str) -> str:
    return f"{rel_path(root, path)}\n{path.stem}\n{text[:500]}"


def required_test_gaps(root: Path) -> list[Gap]:
    gaps: list[Gap] = []
    for rel in P02_TEST_FILES:
        path = root / rel
        if not path.exists():
            gaps.append(Gap(rel, "missing-test", "Required P02 test file is missing."))
            continue
        if not read_text(path).strip():
            gaps.append(Gap(rel, "empty-test", "Required P02 test file is empty."))
    return gaps


def collect_scan_text(root: Path) -> tuple[str, list[EvidenceFile]]:
    chunks: list[str] = []
    files: list[EvidenceFile] = []

    for rel in P02_TEST_FILES:
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
    for label, alternatives in GLOBAL_INVARIANTS:
        if label == "ResultStream metadata/batch/completion":
            if has_all_terms(scan_text, alternatives):
                continue
        elif has_any(scan_text, alternatives):
            continue
        gaps.append(Gap("<P02 evidence corpus>", "invariant", f"Missing P02 invariant `{label}`."))
    return gaps


def report_gaps(root: Path) -> tuple[list[Gap], list[dict[str, object]]]:
    reports = [(path, read_text(path)) for path in existing_report_files(root)]
    summaries: list[dict[str, object]] = []
    gaps: list[Gap] = []

    for report_id, identity_tokens in EXPECTED_REPORTS:
        matches: list[str] = []
        for path, text in reports:
            if has_any(report_identity_text(root, path, text), identity_tokens):
                matches.append(rel_path(root, path))

        summaries.append(
            {
                "id": report_id,
                "matches": matches,
                "satisfied": bool(matches),
            }
        )
        if not matches:
            gaps.append(
                Gap(
                    str(P02_TASK_DIR.as_posix()),
                    "missing-report",
                    f"Missing P02 report `{report_id}`.",
                )
            )

    return gaps, summaries


def build_report(root: Path) -> dict[str, object]:
    root = root.resolve()
    gaps = required_test_gaps(root)
    scan_text, scanned_files = collect_scan_text(root)
    gaps.extend(invariant_gaps(scan_text))
    report_gap_list, report_summaries = report_gaps(root)
    gaps.extend(report_gap_list)

    return {
        "schema": "andromeda.p02_durable_vertical_path.v1",
        "root": str(root),
        "status": "FAIL" if gaps else "PASS",
        "task_dir": P02_TASK_DIR.as_posix(),
        "required_tests": list(P02_TEST_FILES),
        "scanned_files": [asdict(file) for file in scanned_files],
        "required_reports": report_summaries,
        "gaps": [asdict(gap) for gap in gaps],
    }


def print_text(report: dict[str, object]) -> None:
    print("Andromeda P02 Durable Vertical Path")
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
