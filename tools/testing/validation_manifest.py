#!/usr/bin/env python3
"""Read-only local validation manifest aggregator for Andromeda."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Sequence


DEFAULT_ROOT = Path(__file__).resolve().parents[2]

IGNORED_PARTS = {".git", "target"}


@dataclass(frozen=True)
class IndexSpec:
    category: str
    label: str
    path: str
    required: bool = True


@dataclass(frozen=True)
class IndexResult:
    category: str
    label: str
    path: str
    required: bool
    present: bool


@dataclass(frozen=True)
class Blocker:
    category: str
    severity: str
    source: str
    message: str


INDEX_SPECS = (
    IndexSpec("docs", "documentation root index", "documentations/README.md"),
    IndexSpec(
        "docs",
        "reader roadmap index",
        "documentations/00_ANDROMEDA_INDEX_ET_MODE_DE_LECTURE.md",
    ),
    IndexSpec("docs", "Codex documentation index", "docs/codex/README.md"),
    IndexSpec("docs", "architecture index", "documentations/architecture/index.md"),
    IndexSpec(
        "docs",
        "implementation index",
        "documentations/implementation/index.md",
    ),
    IndexSpec(
        "docs",
        "governance decision index",
        "documentations/governance/decisions/index.md",
    ),
    IndexSpec("specs", "specification index", "documentations/specs/index.md"),
    IndexSpec("crates", "crate index", "crates/README.md"),
    IndexSpec("crates", "workspace manifest", "Cargo.toml"),
    IndexSpec("fuzz", "fuzz index", "fuzz/README.md"),
    IndexSpec("fuzz", "fuzz validation matrix", "fuzz/VALIDATION_MATRIX.md"),
    IndexSpec("fuzz", "fuzz target registry", "tests/fuzzing/targets.toml"),
    IndexSpec("fuzz", "fuzz corpus manifest", "tests/fuzzing/corpus/manifest.toml"),
    IndexSpec(
        "runbooks",
        "operations runbook index",
        "documentations/operations/runbooks/index.md",
    ),
    IndexSpec("tests", "test roadmap index", "tests/README.md"),
    IndexSpec("tests", "testing documentation index", "documentations/testing/index.md"),
    IndexSpec(
        "tests",
        "Step 11 validation matrix",
        "documentations/testing/step-11-validation-matrix.md",
    ),
    IndexSpec("tests", "crash/recovery test index", "tests/crash-recovery/README.md"),
    IndexSpec("tests", "fuzzing test index", "tests/fuzzing/README.md"),
    IndexSpec("tests", "Miri test index", "tests/miri/README.md"),
    IndexSpec("tests", "Loom test index", "tests/loom/README.md"),
)


UNCONDITIONAL_STATIC_BLOCKERS = (
    Blocker(
        "tests",
        "high",
        "documentations/testing/step-11-validation-matrix.md",
        "Sustained fuzz evidence remains required before promoted byte, parser, protocol, or admission surfaces can be treated as release evidence.",
    ),
    Blocker(
        "runbooks",
        "critical",
        "documentations/operations/runbooks/index.md",
        "Full backup/restore drills and cluster simulation remain planned gaps; helper scripts do not replace real retained drill evidence.",
    ),
)


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Read-only Andromeda validation manifest aggregator.",
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=DEFAULT_ROOT,
        help="Repository root. Defaults to the root inferred from this script.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit the validation manifest as JSON instead of text.",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit with status 1 when required indices are missing or known blockers are present.",
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


def is_ignored(path: Path) -> bool:
    return any(part in IGNORED_PARTS for part in path.parts)


def safe_rglob(base: Path, pattern: str) -> list[Path]:
    if not base.exists():
        return []
    return sorted(path for path in base.rglob(pattern) if not is_ignored(path))


def count_files(base: Path, pattern: str) -> int:
    return sum(1 for path in safe_rglob(base, pattern) if path.is_file())


def parse_key_values(raw_line: str) -> tuple[str, str] | None:
    line = raw_line.split("#", 1)[0].strip()
    if not line or "=" not in line:
        return None

    key, value = line.split("=", 1)
    key = key.strip()
    value = value.strip()
    quoted = re.match(r'^"([^"]*)"$', value)
    return key, quoted.group(1) if quoted else value


def parse_string_array(value: str) -> tuple[str, ...]:
    return tuple(match.group(1) for match in re.finditer(r'"([^"]*)"', value))


def parse_array_tables(path: Path, table_name: str) -> list[dict[str, object]]:
    tables: list[dict[str, object]] = []
    current: dict[str, object] | None = None
    header = f"[[{table_name}]]"

    for raw_line in read_text(path).splitlines():
        line = raw_line.split("#", 1)[0].strip()
        if not line:
            continue
        if line == header:
            current = {}
            tables.append(current)
            continue
        if line.startswith("[["):
            current = None
            continue
        if current is None:
            continue

        parsed = parse_key_values(raw_line)
        if parsed is None:
            continue
        key, value = parsed
        if value.startswith("[") and value.endswith("]"):
            current[key] = parse_string_array(value)
        else:
            current[key] = value

    return tables


def parse_workspace_members(root: Path) -> tuple[str, ...]:
    text = read_text(root / "Cargo.toml")
    match = re.search(r"members\s*=\s*\[(.*?)\]", text, re.DOTALL)
    if match is None:
        return ()
    members = [item.replace("\\", "/") for item in parse_string_array(match.group(1))]
    return tuple(dict.fromkeys(member for member in members if member.startswith("crates/")))


def evaluate_indices(root: Path) -> list[IndexResult]:
    return [
        IndexResult(
            category=spec.category,
            label=spec.label,
            path=spec.path,
            required=spec.required,
            present=(root / spec.path).exists(),
        )
        for spec in INDEX_SPECS
    ]


def category_summaries(indices: Sequence[IndexResult]) -> list[dict[str, object]]:
    categories = tuple(dict.fromkeys(index.category for index in indices))
    summaries: list[dict[str, object]] = []
    for category in categories:
        category_indices = [index for index in indices if index.category == category]
        missing = [index.path for index in category_indices if not index.present]
        required_missing = [
            index.path
            for index in category_indices
            if index.required and not index.present
        ]
        summaries.append(
            {
                "category": category,
                "total": len(category_indices),
                "present": len(category_indices) - len(missing),
                "missing": missing,
                "required_missing": required_missing,
            }
        )
    return summaries


def index_blockers(indices: Sequence[IndexResult]) -> list[Blocker]:
    return [
        Blocker(
            category=index.category,
            severity="critical",
            source=index.path,
            message=f"Required validation index is missing: {index.label}.",
        )
        for index in indices
        if index.required and not index.present
    ]


def build_docs_inventory(root: Path) -> dict[str, object]:
    documentations = root / "documentations"
    docs = root / "docs"
    return {
        "documentations_markdown_files": count_files(documentations, "*.md"),
        "docs_markdown_files": count_files(docs, "*.md"),
        "top_level_indices": [
            result.path
            for result in evaluate_indices(root)
            if result.category == "docs" and result.present
        ],
    }


def build_specs_inventory(root: Path) -> dict[str, object]:
    specs_dir = root / "documentations" / "specs"
    spec_files = [
        rel(root, path)
        for path in safe_rglob(specs_dir, "*.md")
        if path.name != "index.md"
    ]
    return {
        "specification_files": len(spec_files),
        "index": "documentations/specs/index.md",
        "sample": spec_files[:12],
    }


def build_crates_inventory(root: Path) -> tuple[dict[str, object], list[Blocker]]:
    workspace_members = parse_workspace_members(root)
    crate_dirs = sorted(
        rel(root, path)
        for path in (root / "crates").glob("*")
        if path.is_dir() and not is_ignored(path)
    )
    member_set = set(workspace_members)
    missing_member_manifests = [
        f"{member}/Cargo.toml"
        for member in workspace_members
        if not (root / member / "Cargo.toml").exists()
    ]
    non_workspace_crate_dirs = [
        crate_dir for crate_dir in crate_dirs if crate_dir not in member_set
    ]

    blockers: list[Blocker] = []
    for path in missing_member_manifests:
        blockers.append(
            Blocker(
                "crates",
                "critical",
                path,
                "Workspace member is listed in Cargo.toml but its manifest is missing.",
            )
        )
    for path in non_workspace_crate_dirs:
        blockers.append(
            Blocker(
                "crates",
                "medium",
                path,
                "Crate directory exists but is not listed as a root workspace member.",
            )
        )

    return (
        {
            "workspace_members": len(workspace_members),
            "crate_directories": len(crate_dirs),
            "crate_readmes": count_files(root / "crates", "README.md"),
            "missing_member_manifests": missing_member_manifests,
            "non_workspace_crate_dirs": non_workspace_crate_dirs,
        },
        blockers,
    )


def build_fuzz_inventory(root: Path) -> tuple[dict[str, object], list[Blocker]]:
    targets = parse_array_tables(root / "tests" / "fuzzing" / "targets.toml", "target")
    support = parse_array_tables(root / "tests" / "fuzzing" / "targets.toml", "support")
    bins = parse_array_tables(root / "fuzz" / "Cargo.toml", "bin")
    corpus_entries = parse_array_tables(
        root / "tests" / "fuzzing" / "corpus" / "manifest.toml",
        "entry",
    )
    bin_names = {str(item.get("name", "")) for item in bins}
    support_paths = {str(item.get("path", "")) for item in support if item.get("path")}
    corpus_by_target = {
        str(item.get("target", "")): item for item in corpus_entries if item.get("target")
    }
    blockers: list[Blocker] = []
    registered: list[dict[str, object]] = []

    for target in targets:
        name = str(target.get("name", "<unnamed>"))
        source_value = str(target.get("path", ""))
        corpus_value = str(target.get("corpus_dir", ""))
        generator_value = str(target.get("generator", ""))
        policy = str(target.get("invalid_input_policy", ""))
        source = root / "fuzz" / source_value
        corpus_dir = root / corpus_value
        generator = root / generator_value
        corpus_entry = corpus_by_target.get(name)

        source_present = bool(source_value) and source.exists()
        corpus_present = bool(corpus_value) and corpus_dir.exists()
        generator_present = bool(generator_value) and generator.exists()
        bin_present = name in bin_names
        corpus_manifest_present = corpus_entry is not None

        registered.append(
            {
                "name": name,
                "source": rel(root, source) if source_value else "",
                "source_present": source_present,
                "corpus_dir": corpus_value,
                "corpus_present": corpus_present,
                "generator": generator_value,
                "generator_present": generator_present,
                "cargo_bin_present": bin_present,
                "corpus_manifest_present": corpus_manifest_present,
                "invalid_input_policy": policy,
            }
        )

        for present, value, message in (
            (source_present, source_value, "Fuzz target source is missing."),
            (corpus_present, corpus_value, "Fuzz target corpus directory is missing."),
            (generator_present, generator_value, "Fuzz target seed generator is missing."),
        ):
            if not present:
                blockers.append(
                    Blocker(
                        "fuzz",
                        "high",
                        value or f"tests/fuzzing/targets.toml:{name}",
                        f"{name}: {message}",
                    )
                )
        if not bin_present:
            blockers.append(
                Blocker(
                    "fuzz",
                    "high",
                    "fuzz/Cargo.toml",
                    f"{name}: registered fuzz target has no matching Cargo bin.",
                )
            )
        if not corpus_manifest_present:
            blockers.append(
                Blocker(
                    "fuzz",
                    "high",
                    "tests/fuzzing/corpus/manifest.toml",
                    f"{name}: registered fuzz target has no corpus manifest entry.",
                )
            )
        if "stub" in policy.lower():
            blockers.append(
                Blocker(
                    "fuzz",
                    "medium",
                    "tests/fuzzing/targets.toml",
                    f"{name}: invalid_input_policy records a compile-intent stub and requires owner decode API follow-up before promotion.",
                )
            )

        if corpus_entry is not None and corpus_present:
            seed_files = corpus_entry.get("seed_files", ())
            if isinstance(seed_files, tuple):
                for seed in seed_files:
                    seed_path = corpus_dir / str(seed)
                    if not seed_path.exists():
                        blockers.append(
                            Blocker(
                                "fuzz",
                                "high",
                                rel(root, seed_path),
                                f"{name}: corpus manifest lists a missing seed file.",
                            )
                        )

    source_files = [
        path
        for path in safe_rglob(root / "fuzz" / "fuzz_targets", "*.rs")
        if rel(root / "fuzz", path) not in support_paths
    ]
    return (
        {
            "registered_targets": len(targets),
            "cargo_bins": len(bins),
            "corpus_manifest_entries": len(corpus_entries),
            "target_sources": len(source_files),
            "targets": registered,
        },
        blockers,
    )


def build_runbooks_inventory(root: Path) -> dict[str, object]:
    runbook_dir = root / "documentations" / "operations" / "runbooks"
    runbooks = [
        rel(root, path)
        for path in safe_rglob(runbook_dir, "*.md")
        if path.name != "index.md"
    ]
    return {
        "runbook_files": len(runbooks),
        "index": "documentations/operations/runbooks/index.md",
        "runbooks": runbooks,
    }


def command_inventory(root: Path) -> list[dict[str, object]]:
    command_specs = (
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
    return [
        {
            "label": label,
            "source": path,
            "present": (root / path).exists(),
            "command": command,
        }
        for label, path, command in command_specs
    ]


def find_loom_model_paths(root: Path) -> tuple[str, ...]:
    candidates: set[str] = set()
    cargo_paths = [root / "Cargo.toml", *sorted((root / "crates").glob("*/Cargo.toml"))]
    for path in cargo_paths:
        if "loom" in read_text(path).lower():
            candidates.add(rel(root, path))

    for base in (root / "crates", root / "tests"):
        if not base.exists():
            continue
        for path in safe_rglob(base, "*.rs"):
            text = read_text(path).lower()
            if "loom::" in text or 'feature = "loom"' in text or 'feature="loom"' in text:
                candidates.add(rel(root, path))

    return tuple(sorted(candidates))


def build_tests_inventory(root: Path) -> tuple[dict[str, object], list[Blocker]]:
    root_test_indices = [
        rel(root, path)
        for path in safe_rglob(root / "tests", "README.md")
        if path.is_file()
    ]
    crate_integration_tests = [
        path
        for path in safe_rglob(root / "crates", "*.rs")
        if "tests" in path.parts
    ]
    loom_models = find_loom_model_paths(root)
    blockers: list[Blocker] = []
    if not loom_models:
        blockers.append(
            Blocker(
                "tests",
                "high",
                "tests/loom/README.md",
                "No concrete Loom model path was detected; release claims that depend on concurrency interleavings need owner-crate Loom evidence or an explicit scope exclusion.",
            )
        )

    return (
        {
            "root_test_indices": root_test_indices,
            "root_test_index_count": len(root_test_indices),
            "crate_test_rust_files": len(crate_integration_tests),
            "loom_model_paths": loom_models,
            "evidence_commands": command_inventory(root),
        },
        blockers,
    )


def build_inventory(root: Path) -> tuple[dict[str, object], list[Blocker]]:
    crate_inventory, crate_blockers = build_crates_inventory(root)
    fuzz_inventory, fuzz_blockers = build_fuzz_inventory(root)
    tests_inventory, tests_blockers = build_tests_inventory(root)
    return (
        {
            "docs": build_docs_inventory(root),
            "specs": build_specs_inventory(root),
            "crates": crate_inventory,
            "fuzz": fuzz_inventory,
            "runbooks": build_runbooks_inventory(root),
            "tests": tests_inventory,
        },
        [*crate_blockers, *fuzz_blockers, *tests_blockers],
    )


def has_fuzz_target(root: Path, name: str) -> bool:
    targets = parse_array_tables(root / "tests" / "fuzzing" / "targets.toml", "target")
    return any(str(target.get("name", "")) == name for target in targets)


def static_blockers(root: Path) -> list[Blocker]:
    blockers = list(UNCONDITIONAL_STATIC_BLOCKERS)

    if not has_fuzz_target(root, "segment_index_decode"):
        blockers.append(
            Blocker(
                "fuzz",
                "high",
                "tests/fuzzing/targets.toml",
                "SegmentIndex fuzz coverage is not registered; byte-format promotion requires a real target or explicit scope exclusion.",
            )
        )

    if (root / "tools" / "testing" / "miri_subset.py").exists():
        blockers.append(
            Blocker(
                "tests",
                "high",
                "tools/testing/miri_subset.py",
                "The Miri subset inventory is present, but release approval still requires retained blocking or release-reviewed Miri execution evidence.",
            )
        )
    else:
        blockers.append(
            Blocker(
                "tests",
                "high",
                "documentations/testing/step-11-validation-matrix.md",
                "Blocking or release-recorded Miri evidence remains required for unsafe or memory-sensitive C5 release claims; the nightly workflow is advisory when it uses continue-on-error.",
            )
        )

    if not (root / "tests" / "loom" / "Cargo.toml").exists():
        blockers.append(
            Blocker(
                "tests",
                "high",
                "tests/loom/Cargo.toml",
                "No standalone Loom command is visible; concurrency-sensitive C5 claims require owner-crate Loom evidence or an explicit scope exclusion.",
            )
        )

    if not (root / "crates" / "andromeda-maps" / "tests" / "map_publication_contract.rs").exists():
        blockers.append(
            Blocker(
                "tests",
                "critical",
                "documentations/testing/step-11-validation-matrix.md",
                "Map publication release approval remains blocked until a Map owner suite proves candidate validation, active switch, rollback, rebuild, and recovery behavior.",
            )
        )

    if (root / "crates" / "andromeda-storage" / "tests" / "btree_durable_promotion_contract.rs").exists():
        blockers.append(
            Blocker(
                "crates",
                "high",
                "crates/andromeda-storage/tests/btree_durable_promotion_contract.rs",
                "B-Tree durable promotion has an owner contract path, but release approval still requires retained execution of the contract and combined crash/recovery evidence.",
            )
        )
    else:
        blockers.append(
            Blocker(
                "crates",
                "high",
                "documentations/implementation/v1-gap-closure-tracker.md",
                "B-Tree durable promotion remains blocked until insert, delete, split, merge, WAL replay, and crash recovery tests pass together.",
            )
        )

    if (root / "tools" / "testing" / "release_evidence.py").exists():
        blockers.append(
            Blocker(
                "docs",
                "high",
                "tools/testing/release_evidence.py",
                "The release evidence generator is present, but release approval still requires exact gate commands, commit SHA, toolchain, pass/fail status, skipped tests, retained artifacts, and unresolved gaps.",
            )
        )
    else:
        blockers.append(
            Blocker(
                "docs",
                "high",
                "documentations/implementation/v1-gap-closure-tracker.md",
                "Release approval still requires exact gate commands, commit SHA, toolchain, pass/fail status, skipped tests, and unresolved gaps.",
            )
        )

    return blockers


def dedupe_blockers(blockers: Sequence[Blocker]) -> list[Blocker]:
    return list(
        {
            (blocker.category, blocker.severity, blocker.source, blocker.message): blocker
            for blocker in blockers
        }.values()
    )


def status(indices: Sequence[IndexResult], blockers: Sequence[Blocker]) -> str:
    if any(index.required and not index.present for index in indices):
        return "FAIL"
    if blockers:
        return "BLOCKED"
    return "PASS"


def build_report(root: Path) -> dict[str, object]:
    root = root.resolve()
    indices = evaluate_indices(root)
    inventory, inventory_blockers = build_inventory(root)
    blockers = dedupe_blockers(
        [
            *index_blockers(indices),
            *inventory_blockers,
            *static_blockers(root),
        ]
    )
    report_status = status(indices, blockers)
    return {
        "schema": "andromeda.validation_manifest.v1",
        "root": str(root),
        "status": report_status,
        "summary": category_summaries(indices),
        "indices": [asdict(index) for index in indices],
        "inventory": inventory,
        "known_blockers": [asdict(blocker) for blocker in blockers],
    }


def print_text_report(report: dict[str, object]) -> None:
    print("Andromeda Validation Manifest")
    print(f"Repository: {report['root']}")
    print(f"Status: {report['status']}")
    print()

    print("Index Presence")
    for raw_summary in report["summary"]:
        assert isinstance(raw_summary, dict)
        print(
            f"  {raw_summary['category']}: "
            f"{raw_summary['present']}/{raw_summary['total']} present"
        )
        for path in raw_summary["missing"]:
            print(f"    missing: {path}")
    print()

    inventory = report["inventory"]
    assert isinstance(inventory, dict)
    print("Inventory")
    for category in ("docs", "specs", "crates", "fuzz", "runbooks", "tests"):
        raw_item = inventory[category]
        assert isinstance(raw_item, dict)
        print(f"  {category}:")
        for key, value in raw_item.items():
            if key == "targets":
                print(f"    targets: {len(value)} registered")
                continue
            if isinstance(value, list | tuple):
                sample = list(value)[:8]
                print(f"    {key}: {len(value)}")
                for item in sample:
                    print(f"      {item}")
                if len(value) > len(sample):
                    print(f"      ... {len(value) - len(sample)} more")
            else:
                print(f"    {key}: {value}")
    print()

    blockers = report["known_blockers"]
    assert isinstance(blockers, list)
    print("Known Blockers")
    if not blockers:
        print("  none")
    for raw_blocker in blockers:
        assert isinstance(raw_blocker, dict)
        print(
            f"  [{raw_blocker['severity']}] {raw_blocker['category']} - "
            f"{raw_blocker['source']}: {raw_blocker['message']}"
        )
    print()
    print("Result: validation manifest completed.")


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    report = build_report(args.root)

    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print_text_report(report)

    return 1 if args.strict and report["status"] != "PASS" else 0


if __name__ == "__main__":
    sys.exit(main())
