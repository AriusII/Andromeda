#!/usr/bin/env python3
"""Read-only P01 normative specification baseline checker."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Sequence


DEFAULT_ROOT = Path(__file__).resolve().parents[2]

COMMON_SECTIONS = (
    "## Purpose",
    "## Scope",
    "## Non-goals",
    "## Data structures",
    "## Invariants",
    "## Serialization",
    "## State transitions",
    "## Error model",
    "## Security model",
    "## Observability",
    "## Recovery behavior",
    "## Compatibility",
    "## Tests",
    "## Rejection criteria",
    "## Acceptance summary",
)


@dataclass(frozen=True)
class RequiredSpec:
    path: str
    tokens: tuple[str, ...]


@dataclass(frozen=True)
class Gap:
    path: str
    category: str
    message: str


REQUIRED_SPECS = (
    RequiredSpec(
        "docs/specifications/SPEC_PROCEDURE_CONTRACT_V0.md",
        (
            "InputShape",
            "OutputShape",
            "ReadSet",
            "WriteSet",
            "RequiredPermissions",
            "IsolationPolicy",
            "ResourcePolicy",
            "ProtocolLayout",
            "CompatibilityPolicy",
            "ContractHash",
            "Canonical contract hash form",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_TYPE_SYSTEM_V0.md",
        (
            "DecimalExact",
            "FloatPolicy",
            "TextPolicy",
            "OptionalPolicy",
            "Cardinality",
            "ambient NULL",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_SRPL_GRAMMAR_V0.md",
        (
            "No dynamic SQL",
            "reserved for_each",
            "diagnostic_code",
            "bounded_loop_policy",
            "shape-changing",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_SRPL_BINDER_V0.md",
        (
            "NameResolution",
            "CardinalityBinding",
            "ReadWriteSet",
            "AbsenceBinding",
            "StableDiagnostic",
            "WAL-covered",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_SEMANTIC_IR_V0.md",
        (
            "IrCardinality",
            "IrAbsence",
            "set semantics",
            "cardinality",
            "absence",
            "native Rust layout",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_CATALOG_OBJECT_MODEL_V0.md",
        (
            "CatalogVersion",
            "CatalogObjectId",
            "ContractHashBinding",
            "PublicationEvidence",
            "durable publication evidence",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_DEFINITION_BATCH_V0.md",
        (
            "DefinitionBatchId",
            "CompatibilityDecision",
            "PublicationWalRecord",
            "DurableLsn",
            "Durable publication evidence",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_WAL_RECORD_V0.md",
        (
            "WalRecordFrameV0 byte layout",
            "PrevLsn",
            "RecordLengthInv",
            "ChainHash",
            "TxBegin",
            "RowInsert",
            "TxCommit",
            "72 bytes",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_FILE_WAL_SEGMENT_V0.md",
        (
            "DurablePrefix",
            "FlushThroughLsn",
            "flush_through",
            "visible commit",
            "durable prefix",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_PAGE_FORMAT_V0.md",
        (
            "PageCodecV1 layout",
            "PageMagic",
            "FormatVersion",
            "PageLsn",
            "112-byte",
            "48-byte trailer",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_DATABASE_MANIFEST_V0.md",
        (
            "DatabaseManifestV0 canonical shape",
            "ManifestVersion",
            "ManifestHash",
            "Signature",
            "RequiredWalStartLsn",
            "root switch",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_SEGMENT_INDEX_V0.md",
        (
            "SegmentIndexV0 file layout",
            "ANDSGIX0",
            "256-byte",
            "160-byte",
            "96-byte",
            "EntryTableSha256",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_TRANSACTION_STATE_MACHINE_V0.md",
        (
            "DurableCommitEvidence",
            "RollbackEvidence",
            "VisibleCommitFence",
            "flush_through",
            "TransactionRejectionCode",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_RECOVERY_REPORT_V0.md",
        (
            "RecoveryReportV0 schema",
            "LastValidWalLsn",
            "DurablePrefixBytes",
            "BoundaryKind",
            "ForensicOnly",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_CRASH_RECOVERY_TEST_PLAN_V0.md",
        (
            "CrashScenarioId",
            "RecoveryReport",
            "OpenModeDecision",
            "CrashEvidenceRecord",
            "visible commit",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_RPC_FRAME_V0.md",
        (
            "FrameHeader v0 wire layout",
            "FrameType",
            "PayloadLength",
            "SurfaceScope",
            "ProtobufEnvelope",
            "gRPC",
            "JSON-native",
            "52 bytes",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_RESULT_STREAM_V0.md",
        (
            "ResultMetadata",
            "ResultPayloadFrame",
            "ResultCompletion",
            "StreamRole",
            "metadata precedes",
            "single-completion",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_SECURITY_ADMISSION_V0.md",
        (
            "CertificateIdentity",
            "UserPrincipal",
            "Permission",
            "PolicyVersion",
            "BreakGlassPolicy",
            "Fail-closed decision matrix",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_AUDIT_LEDGER_V0.md",
        (
            "AuditRecordHeader",
            "AuditLedgerVersion",
            "PolicyVersion",
            "ChainHash",
            "Durable audit record format",
        ),
    ),
    RequiredSpec(
        "docs/specifications/SPEC_DECISION_TRACE_V0.md",
        (
            "DecisionTrace",
            "DecisionKind",
            "DecisionReasonCode",
            "DecisionEvidenceRef",
            "trace as durable truth",
        ),
    ),
)

NEGATED_OR_DISAMBIGUATING = (
    "no ",
    "not ",
    "reject ",
    "rejects ",
    "rejection",
    "forbid",
    "forbidden",
    "without ",
    "does not authorize",
    "never ",
    "non-goal",
)

WATCH_TERMS = ("dynamic sql", "native-layout serialization", "null ambient", "query", "queries", "view", "views")
GENERIC_ACCEPTANCE_SUMMARY = (
    "This specification is acceptable when implementation, tests, and documentation can prove "
    "the listed invariants without hidden defaults."
)


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


def has_token(text: str, token: str) -> bool:
    return token.lower() in text.lower()


def has_watch_term(text: str, term: str) -> bool:
    if term in {"query", "queries", "view", "views"}:
        return re.search(rf"\b{re.escape(term)}\b", text, flags=re.IGNORECASE) is not None
    return term.lower() in text.lower()


def section_after(text: str, heading: str) -> str:
    start = text.find(heading)
    if start == -1:
        return ""
    body_start = start + len(heading)
    next_heading = text.find("\n## ", body_start)
    if next_heading == -1:
        return text[body_start:].strip()
    return text[body_start:next_heading].strip()


def required_spec_gaps(root: Path) -> list[Gap]:
    gaps: list[Gap] = []
    index = read_text(root / "docs/specifications/README.md")

    for spec in REQUIRED_SPECS:
        path = root / spec.path
        text = read_text(path)
        if not path.exists():
            gaps.append(Gap(spec.path, "missing-spec", "Required P01 spec is missing."))
            continue

        if Path(spec.path).name not in index:
            gaps.append(Gap(spec.path, "index", "Required P01 spec is not indexed."))

        for section in COMMON_SECTIONS:
            if section not in text:
                gaps.append(Gap(spec.path, "section", f"Missing section `{section}`."))

        if "- Reject `" not in text and "- Reject " not in text:
            gaps.append(Gap(spec.path, "rejection", "Spec has no explicit Reject entries."))

        for token in spec.tokens:
            if not has_token(text, token):
                gaps.append(Gap(spec.path, "token", f"Missing required token `{token}`."))

    return gaps


def template_gaps(root: Path) -> list[Gap]:
    path = root / "docs/templates/SPEC_TEMPLATE.md"
    text = read_text(path)
    gaps: list[Gap] = []
    for section in (*COMMON_SECTIONS,):
        if section not in text:
            gaps.append(Gap("docs/templates/SPEC_TEMPLATE.md", "template", f"Missing `{section}`."))
    for token in ("Native Rust layout is not a valid format", "Acceptance summary"):
        if not has_token(text, token):
            gaps.append(Gap("docs/templates/SPEC_TEMPLATE.md", "template", f"Missing `{token}`."))
    for token in ("Owner crates", "Acceptance evidence"):
        if not has_token(text, token):
            gaps.append(Gap("docs/templates/SPEC_TEMPLATE.md", "template", f"Missing `{token}`."))
    return gaps


def terminology_gaps(root: Path) -> list[Gap]:
    gaps: list[Gap] = []
    for path in sorted((root / "docs/specifications").glob("SPEC_*_V0.md")):
        text = read_text(path)
        for line_no, line in enumerate(text.splitlines(), start=1):
            lowered = line.lower()
            for term in WATCH_TERMS:
                if not has_watch_term(lowered, term):
                    continue
                if any(marker in lowered for marker in NEGATED_OR_DISAMBIGUATING):
                    continue
                rel = path.relative_to(root).as_posix()
                gaps.append(
                    Gap(
                        f"{rel}:{line_no}",
                        "terminology",
                        f"Ambiguous `{term}` usage is not explicitly negated or disambiguated.",
                    )
                )
    return gaps


def acceptance_summary_gaps(root: Path) -> list[Gap]:
    gaps: list[Gap] = []
    for spec in REQUIRED_SPECS:
        path = root / spec.path
        text = read_text(path)
        if not text:
            continue
        summary = section_after(text, "## Acceptance summary")
        if GENERIC_ACCEPTANCE_SUMMARY.lower() in summary.lower():
            gaps.append(
                Gap(
                    spec.path,
                    "acceptance-summary",
                    "Acceptance summary is generic; it must name owner, evidence, and rejection proof.",
                )
            )
            continue
        for token in ("Owner", "Evidence", "Reject"):
            if not has_token(summary, token):
                gaps.append(
                    Gap(
                        spec.path,
                        "acceptance-summary",
                        f"Acceptance summary must include `{token}`.",
                    )
                )
    return gaps


def build_report(root: Path) -> dict[str, object]:
    root = root.resolve()
    gaps = [
        *required_spec_gaps(root),
        *template_gaps(root),
        *terminology_gaps(root),
        *acceptance_summary_gaps(root),
    ]
    return {
        "schema": "andromeda.p01_spec_baseline.v1",
        "root": str(root),
        "status": "FAIL" if gaps else "PASS",
        "required_specs": [spec.path for spec in REQUIRED_SPECS],
        "gaps": [asdict(gap) for gap in gaps],
    }


def print_text(report: dict[str, object]) -> None:
    print("Andromeda P01 Specification Baseline")
    print(f"Repository: {report['root']}")
    print(f"Status: {report['status']}")
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
