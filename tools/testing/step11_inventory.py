#!/usr/bin/env python3
"""Read-only Step 11 roadmap validation inventory for Andromeda."""

from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


@dataclass(frozen=True)
class PathCheck:
    label: str
    patterns: tuple[str, ...]


@dataclass(frozen=True)
class CheckResult:
    label: str
    found: list[Path]
    missing: list[str]


DOC_CHECKS = (
    PathCheck(
        "test roadmap documentation",
        (
            "tests/README.md",
            "tests/AGENTS.md",
            "documentations/testing/step-11-validation-matrix.md",
            "docs/codex/rust-critical-quality-gates.md",
            "docs/codex/mission-critical-change-policy.md",
            "fuzz/README.md",
            "fuzz/VALIDATION_MATRIX.md",
        ),
    ),
)


RUNBOOK_CHECKS = (
    PathCheck(
        "operations runbooks",
        (
            "documentations/operations/runbooks/index.md",
            "documentations/operations/runbooks/restore-pitr-drill.md",
            "documentations/operations/runbooks/corruption-suspicion.md",
            "documentations/operations/runbooks/replica-lag.md",
            "documentations/operations/runbooks/slow-client.md",
            "documentations/operations/runbooks/wal-pressure.md",
        ),
    ),
)


WORKFLOW_CHECKS = (
    PathCheck(
        "quality and release workflows",
        (
            ".github/workflows/00-ci.yml",
            ".github/workflows/01-rust-matrix.yml",
            ".github/workflows/05-supply-chain.yml",
            ".github/workflows/06-nightly-deep-validation.yml",
            ".github/workflows/07-fuzzing.yml",
            ".github/workflows/15-crash-recovery-placeholder.yml",
            ".github/workflows/16-protocol-doctrine-scan.yml",
            ".github/workflows/release-gate-chain.yml",
            ".github/workflows/perf-regression.yml",
        ),
    ),
)


EVIDENCE_CHECKS = (
    PathCheck(
        "fuzz evidence paths",
        (
            "tests/fuzzing/targets.toml",
            "fuzz/VALIDATION_MATRIX.md",
            "fuzz/README.md",
            "fuzz/fuzz_targets/*.rs",
            "tests/fuzzing/corpus/manifest.toml",
            "fuzz/generators/generate_seed_corpus.py",
            ".github/workflows/07-fuzzing.yml",
        ),
    ),
    PathCheck(
        "Miri evidence paths",
        (
            ".github/workflows/06-nightly-deep-validation.yml",
            "tools/testing/miri_subset.py",
            "documentations/testing/miri-subset-2026-05-08.md",
            "docs/codex/rust-critical-quality-gates.md",
            "documentations/testing/step-11-validation-matrix.md",
        ),
    ),
    PathCheck(
        "Loom evidence paths",
        (
            ".github/workflows/06-nightly-deep-validation.yml",
            "tests/loom/README.md",
            "tests/loom/Cargo.toml",
            "tests/loom/tests/*.rs",
            "documentations/testing/step-11-validation-matrix.md",
        ),
    ),
    PathCheck(
        "crash and recovery evidence paths",
        (
            ".github/workflows/15-crash-recovery-placeholder.yml",
            "crates/andromeda-storage/tests/crash_recovery_impl.rs",
            "crates/andromeda-storage/tests/recovery_completeness_contract.rs",
            "crates/andromeda-storage/tests/property_recovery_replay.rs",
            "crates/andromeda-storage/tests/wal_scan_recovery_contract.rs",
            "crates/andromeda-storage/tests/file_wal_recovery_contract.rs",
            "crates/andromeda-exec/tests/recovery_visibility_gates.rs",
            "documentations/operations/runbooks/restore-pitr-drill.md",
        ),
    ),
    PathCheck(
        "release evidence helper paths",
        (
            "tools/testing/release_evidence.py",
            "tools/testing/release_evidence_schema.md",
            "documentations/testing/release-evidence-template.md",
            "documentations/testing/ci-release-gate-evidence.md",
        ),
    ),
)


ROADMAP_GATE_CHECKS = (
    PathCheck(
        "tests/integration",
        (
            "crates/andromeda-exec/tests/integration_execution_path.rs",
            "crates/andromeda-exec/tests/v0_vertical_e2e.rs",
            "crates/andromeda-exec/tests/inventory_runtime_e2e_gates.rs",
            "crates/andromeda-exec/tests/runtime_contract.rs",
            "crates/andromeda-exec/tests/executor_validation_gates.rs",
            "crates/andromeda-exec/tests/metadata_extraction_contract.rs",
            "crates/andromeda-exec/tests/tx_commit_log_wal_bridge.rs",
            "crates/andromeda-exec/tests/c4_admission_events.rs",
            "crates/andromeda-exec/tests/c5_commit_rollback_lifecycle.rs",
            "crates/andromeda-exec/tests/c5_combined_release_gate.rs",
        ),
    ),
    PathCheck(
        "tests/recovery",
        (
            "crates/andromeda-wal/tests/*.rs",
            "crates/andromeda-storage/tests/crash_recovery_impl.rs",
            "crates/andromeda-storage/tests/recovery_completeness_contract.rs",
            "crates/andromeda-storage/tests/property_recovery_replay.rs",
            "crates/andromeda-storage/tests/wal_scan_recovery_contract.rs",
            "crates/andromeda-storage/tests/file_wal_recovery_contract.rs",
            "crates/andromeda-storage/tests/recovery_replay_heap_redo_contract.rs",
            "crates/andromeda-storage/tests/recovery_replay_page_records_contract.rs",
            "crates/andromeda-storage/tests/wal_durability_fence_contract.rs",
            "crates/andromeda-storage/tests/disk_manager_durability_crash_safety.rs",
            "crates/andromeda-exec/tests/recovery_visibility_gates.rs",
            "crates/andromeda-tx/tests/commit_log_durability.rs",
            "crates/andromeda-tx/tests/tx_wal_replay_recovery.rs",
            "crates/andromeda-tx/tests/storage_tx_wal_adapter_contract.rs",
        ),
    ),
    PathCheck(
        "tests/rpc",
        (
            "crates/andromeda-rpc-protocol/tests/frame_wire_contract.rs",
            "crates/andromeda-rpc-protocol/tests/forbidden_surface_drift.rs",
            "crates/andromeda-quic/tests/codec_contract.rs",
            "crates/andromeda-quic/tests/protocol_stability_contract.rs",
            "crates/andromeda-quic/tests/protobuf_projection_contract.rs",
            "crates/andromeda-quic/tests/procedure_gateway_route.rs",
            "crates/andromeda-quic/tests/transport_contract.rs",
            "crates/andromeda-quic/tests/zero_rtt_admission_policy.rs",
            "crates/andromeda-quic/tests/certificate_continuity_contract.rs",
            "crates/andromeda-exec/tests/remote_invoke_network_e2e.rs",
            "crates/andromeda-exec/tests/result_stream_backpressure.rs",
        ),
    ),
    PathCheck(
        "tests/srpl",
        (
            "crates/andromeda-srpl/tests/validation_gates.rs",
            "crates/andromeda-srpl/tests/compiler_pipeline_e2e.rs",
            "crates/andromeda-srpl/tests/definitionbatch_compat.rs",
            "crates/andromeda-srpl/tests/property_parser_fuzz.rs",
            "crates/andromeda-srpl/tests/optimizer_*.rs",
            "crates/andromeda-srpl-parser/tests/owner_direct.rs",
            "crates/andromeda-srpl-ast/tests/owner_direct.rs",
        ),
    ),
    PathCheck(
        "tests/storage",
        (
            "crates/andromeda-storage/tests/storage_hotcold_pipeline_e2e.rs",
            "crates/andromeda-storage/tests/page_ownership_invariants.rs",
            "crates/andromeda-storage/tests/heap_*.rs",
            "crates/andromeda-storage/tests/btree_*.rs",
            "crates/andromeda-storage/tests/btree_durable_promotion_contract.rs",
            "crates/andromeda-storage/tests/property_*.rs",
            "crates/andromeda-storage/tests/wal_*.rs",
            "crates/andromeda-storage/tests/backup_*.rs",
            "crates/andromeda-storage/tests/hadr_*.rs",
            "crates/andromeda-storage/tests/quorum_*.rs",
            "crates/andromeda-wal/tests/*.rs",
        ),
    ),
    PathCheck(
        "tests/security",
        (
            "crates/andromeda-security-contract/src/*.rs",
            "crates/andromeda-core/tests/principal_integration.rs",
            "crates/andromeda-contract/tests/contract_hash_golden.rs",
            "crates/andromeda-exec/tests/iam_pipeline_e2e.rs",
            "crates/andromeda-exec/tests/iam_hardening.rs",
            "crates/andromeda-exec/tests/permission_scope_contract.rs",
            "crates/andromeda-exec/tests/exec_audit_completion_validation.rs",
            "crates/andromeda-quic/tests/certificate_continuity_contract.rs",
            "crates/andromeda-quic/tests/zero_rtt_admission_policy.rs",
            "crates/andromeda-quic/tests/procedure_gateway_route.rs",
        ),
    ),
    PathCheck(
        "tests/maps",
        (
            "crates/andromeda-maps/Cargo.toml",
            "crates/andromeda-maps/tests/map_publication_contract.rs",
        ),
    ),
)


KNOWN_RELEASE_GAPS = (
    "recorded sustained fuzz run evidence for promoted byte, parser, protocol, and admission surfaces",
    "blocking Miri release gate or recorded passing Miri subset for memory-sensitive crates",
    "release-recorded c5_combined_release_gate run tying Procedure admission, ContractHash rejection, WAL append, durable completion, ResultStream metadata, audit, and recovery visibility",
    "release artifact with exact commands, commit SHA, toolchain, pass/fail status, skipped tests, and unresolved gaps",
    "real backup/restore and cluster failover drill evidence with retained artifacts, not only helper scripts or runbooks",
)


EVIDENCE_COMMANDS = (
    (
        "C5 combined release gate",
        "crates/andromeda-exec/tests/c5_combined_release_gate.rs",
        "cargo test -p andromeda-exec --test c5_combined_release_gate --locked -- --nocapture",
    ),
    (
        "B-Tree durable promotion contract",
        "crates/andromeda-storage/tests/btree_durable_promotion_contract.rs",
        "cargo test -p andromeda-storage --test btree_durable_promotion_contract --locked -- --nocapture",
    ),
    (
        "Map publication contract",
        "crates/andromeda-maps/tests/map_publication_contract.rs",
        "cargo test -p andromeda-maps --test map_publication_contract --locked -- --nocapture",
    ),
    (
        "SegmentIndex fuzz target compile check",
        "fuzz/fuzz_targets/segment_index_decode.rs",
        "cargo check --manifest-path fuzz/Cargo.toml --bin segment_index_decode --locked",
    ),
    (
        "standalone Loom smoke model",
        "tests/loom/Cargo.toml",
        "cargo test --manifest-path tests/loom/Cargo.toml",
    ),
    (
        "Miri subset inventory",
        "tools/testing/miri_subset.py",
        "python -B tools/testing/miri_subset.py --show-exclusions",
    ),
    (
        "release evidence generator",
        "tools/testing/release_evidence.py",
        "python -B tools/testing/release_evidence.py --json",
    ),
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Read-only Step 11 roadmap validation inventory.",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit with status 1 when inventory gaps are found.",
    )
    return parser.parse_args()


def rel(path: Path) -> str:
    try:
        return path.relative_to(ROOT).as_posix()
    except ValueError:
        return str(path)


def matches(pattern: str) -> list[Path]:
    pattern_path = ROOT / pattern
    if any(char in pattern for char in "*?[]"):
        return sorted(ROOT.glob(pattern))
    if pattern_path.exists():
        return [pattern_path]
    return []


def evaluate(check: PathCheck) -> CheckResult:
    found: list[Path] = []
    missing: list[str] = []
    for pattern in check.patterns:
        resolved = matches(pattern)
        if resolved:
            found.extend(resolved)
        else:
            missing.append(pattern)
    return CheckResult(check.label, list(dict.fromkeys(found)), missing)


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        return ""


def parse_fuzz_targets(path: Path) -> list[dict[str, str]]:
    targets: list[dict[str, str]] = []
    current: dict[str, str] | None = None
    if not path.exists():
        return targets

    for raw_line in read_text(path).splitlines():
        line = raw_line.split("#", 1)[0].strip()
        if not line:
            continue
        if line == "[[target]]":
            current = {}
            targets.append(current)
            continue
        if line.startswith("[["):
            current = None
            continue
        if current is None or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()
        quoted = re.match(r'^"([^"]*)"$', value)
        current[key] = quoted.group(1) if quoted else value
    return targets


def print_check_group(title: str, checks: tuple[PathCheck, ...]) -> list[CheckResult]:
    print(title)
    results = [evaluate(check) for check in checks]
    for result in results:
        print(f"  {result.label}: {len(result.found)} found, {len(result.missing)} missing")
        for path in result.found[:12]:
            print(f"    found: {rel(path)}")
        if len(result.found) > 12:
            print(f"    found: ... {len(result.found) - 12} more")
        for pattern in result.missing:
            print(f"    missing: {pattern}")
    print()
    return results


def print_fuzz_targets() -> list[str]:
    print("Fuzz Targets")
    targets_path = ROOT / "tests" / "fuzzing" / "targets.toml"
    targets = parse_fuzz_targets(targets_path)
    missing: list[str] = []
    print(f"  registry: {rel(targets_path)}")
    print(f"  registered targets: {len(targets)}")
    for target in targets:
        name = target.get("name", "<unnamed>")
        target_file = ROOT / "fuzz" / target.get("path", "")
        corpus_dir = ROOT / target.get("corpus_dir", "")
        generator = ROOT / target.get("generator", "")
        status_parts = [
            f"source={'found' if target_file.exists() else 'missing'}",
            f"corpus={'found' if corpus_dir.exists() else 'missing'}",
            f"generator={'found' if generator.exists() else 'missing'}",
        ]
        print(f"    {name}: {', '.join(status_parts)}")
        if not target_file.exists():
            missing.append(f"fuzz target source for {name}: {rel(target_file)}")
        if not corpus_dir.exists():
            missing.append(f"fuzz corpus for {name}: {rel(corpus_dir)}")
        if not generator.exists():
            missing.append(f"fuzz generator for {name}: {rel(generator)}")
    print()
    return missing


def find_loom_index_paths() -> list[Path]:
    paths = [ROOT / "tests" / "loom" / "README.md"]
    return [path for path in paths if path.exists()]


def find_loom_command_paths() -> list[Path]:
    paths = [ROOT / "tests" / "loom" / "Cargo.toml"]
    return [path for path in paths if path.exists()]


def find_loom_model_paths() -> list[Path]:
    candidates: list[Path] = []
    cargo_paths = [
        ROOT / "Cargo.toml",
        ROOT / "tests" / "loom" / "Cargo.toml",
        *sorted((ROOT / "crates").glob("*/Cargo.toml")),
    ]
    for cargo_toml in cargo_paths:
        if "loom" in read_text(cargo_toml).lower():
            candidates.append(cargo_toml)

    for base in (ROOT / "crates", ROOT / "tests"):
        if not base.exists():
            continue
        for path in base.rglob("*.rs"):
            if "target" in path.parts:
                continue
            text = read_text(path).lower()
            if "loom::" in text or "feature = \"loom\"" in text or "feature=\"loom\"" in text:
                candidates.append(path)
    return sorted(set(candidates))


def print_miri_loom_notes() -> list[str]:
    print("Miri and Loom")
    missing: list[str] = []
    miri_workflow = ROOT / ".github" / "workflows" / "06-nightly-deep-validation.yml"
    miri_text = read_text(miri_workflow).lower()
    if miri_workflow.exists() and "miri" in miri_text:
        mode = "non-blocking smoke" if "continue-on-error: true" in miri_text else "blocking"
        print(f"  Miri workflow: found ({mode}) - {rel(miri_workflow)}")
        if mode == "non-blocking smoke":
            missing.append(
                "blocking Miri release gate or recorded passing Miri subset for memory-sensitive crates"
            )
    else:
        print("  Miri workflow: missing")
        missing.append("Miri workflow evidence")

    miri_subset = ROOT / "tools" / "testing" / "miri_subset.py"
    if miri_subset.exists():
        print(f"  Miri subset helper: found - {rel(miri_subset)}")
        print("    command: python -B tools/testing/miri_subset.py --show-exclusions")
    else:
        print("  Miri subset helper: missing")
        missing.append("Miri subset helper")

    loom_index_paths = find_loom_index_paths()
    if loom_index_paths:
        print(f"  Loom index paths: {len(loom_index_paths)} found")
        for path in loom_index_paths:
            print(f"    found: {rel(path)}")
    else:
        print("  Loom index paths: none found")

    loom_command_paths = find_loom_command_paths()
    if loom_command_paths:
        print(f"  Loom standalone command paths: {len(loom_command_paths)} found")
        for path in loom_command_paths:
            print(f"    found: {rel(path)}")
        print("    command: cargo test --manifest-path tests/loom/Cargo.toml")
    else:
        print("  Loom standalone command paths: none found")
        missing.append("standalone Loom command")

    loom_model_paths = find_loom_model_paths()
    if loom_model_paths:
        print(f"  Loom model evidence paths: {len(loom_model_paths)} found")
        for path in loom_model_paths[:12]:
            print(f"    found: {rel(path)}")
        if len(loom_model_paths) > 12:
            print(f"    found: ... {len(loom_model_paths) - 12} more")
    else:
        print("  Loom model evidence paths: none found")
        missing.append("Loom model evidence")
    print()
    return missing


def print_evidence_commands() -> list[str]:
    print("New Evidence Commands")
    missing: list[str] = []
    for label, source, command in EVIDENCE_COMMANDS:
        path = ROOT / source
        status = "found" if path.exists() else "missing"
        print(f"  {label}: {status} - {source}")
        print(f"    command: {command}")
        if not path.exists():
            missing.append(f"{label}: {source}")
    print()
    return missing


def print_missing_roadmap_gates(
    results: list[CheckResult],
    fuzz_missing: list[str],
    miri_loom_missing: list[str],
    evidence_command_missing: list[str],
) -> list[str]:
    missing: list[str] = []
    for result in results:
        for pattern in result.missing:
            missing.append(f"{result.label}: {pattern}")
    missing.extend(fuzz_missing)
    missing.extend(miri_loom_missing)
    missing.extend(evidence_command_missing)
    missing.extend(KNOWN_RELEASE_GAPS)

    deduped = list(dict.fromkeys(missing))
    print("Missing Roadmap Gates")
    if not deduped:
        print("  none")
    for item in deduped:
        print(f"  {item}")
    print()
    return deduped


def main() -> int:
    args = parse_args()
    print("Andromeda Step 11 Inventory")
    print(f"Repository: {ROOT}")
    print()

    doc_results = print_check_group("Test Documentation", DOC_CHECKS)
    runbook_results = print_check_group("Runbooks", RUNBOOK_CHECKS)
    workflow_results = print_check_group("Workflows", WORKFLOW_CHECKS)
    evidence_results = print_check_group("Evidence Paths", EVIDENCE_CHECKS)
    roadmap_results = print_check_group("Roadmap Gate Paths", ROADMAP_GATE_CHECKS)
    fuzz_missing = print_fuzz_targets()
    miri_loom_missing = print_miri_loom_notes()
    evidence_command_missing = print_evidence_commands()

    missing = print_missing_roadmap_gates(
        [
            *doc_results,
            *runbook_results,
            *workflow_results,
            *evidence_results,
            *roadmap_results,
        ],
        fuzz_missing,
        miri_loom_missing,
        evidence_command_missing,
    )
    print("Result: inventory completed.")
    return 1 if args.strict and missing else 0


if __name__ == "__main__":
    raise SystemExit(main())
