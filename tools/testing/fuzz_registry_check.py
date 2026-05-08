#!/usr/bin/env python3
from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tomllib
from typing import Any


TARGETS_SCHEMA_VERSION = "andromeda-fuzz-targets-v1"
CORPUS_SCHEMA_VERSION = "andromeda-fuzz-corpus-v1"
GENERATOR_PATH = "fuzz/generators/generate_seed_corpus.py"
TARGETS_REGISTRY_PATH = "tests/fuzzing/targets.toml"
CORPUS_MANIFEST_PATH = "tests/fuzzing/corpus/manifest.toml"
TARGET_PATH_PREFIX = "fuzz_targets/"
CORPUS_DIR_PREFIX = "tests/fuzzing/corpus/"
WORKFLOW_PATH = ".github/workflows/07-fuzzing.yml"
CANONICAL_RUN_COMMAND = (
    'cargo +nightly fuzz run "$target" "$corpus_dir" -- '
    '-max_total_time="$fuzz_seconds"'
)
ALLOWED_RUNTIME_OUTPUT_PREFIXES = ("fuzz/target/",)
RUNTIME_HYGIENE_EXEMPT_PATH_PREFIXES = ("fuzz/generators/",)
FORBIDDEN_RUNTIME_DIR_NAMES = frozenset({"__pycache__", "artifacts", "coverage", "crashes"})
FORBIDDEN_RUNTIME_FILE_PREFIXES = ("crash-", "leak-", "oom-", "slow-unit-", "timeout-")
FORBIDDEN_RUNTIME_FILE_SUFFIXES = (".profraw", ".pyc", ".sancov")


@dataclass(frozen=True)
class TargetSpec:
    name: str
    path: str
    corpus_dir: str
    generator: str
    max_input_bytes: int
    invalid_input_policy: str


@dataclass(frozen=True)
class SupportSpec:
    path: str
    used_by: tuple[str, ...]


@dataclass(frozen=True)
class CargoBinSpec:
    name: str
    path: str
    test: bool
    doc: bool


@dataclass(frozen=True)
class ManifestEntry:
    target: str
    corpus_dir: str
    seed_files: tuple[str, ...]
    generator: str


@dataclass(frozen=True)
class Registry:
    targets: tuple[TargetSpec, ...]
    support: tuple[SupportSpec, ...]
    bins: tuple[CargoBinSpec, ...]
    manifest_entries: tuple[ManifestEntry, ...]


def load_toml(path: Path, errors: list[str]) -> dict[str, Any]:
    if not path.is_file():
        errors.append(f"missing file: {path.as_posix()}")
        return {}
    try:
        with path.open("rb") as handle:
            loaded = tomllib.load(handle)
    except tomllib.TOMLDecodeError as exc:
        errors.append(f"{path.as_posix()}: invalid TOML: {exc}")
        return {}
    if not isinstance(loaded, dict):
        errors.append(f"{path.as_posix()}: TOML root must be a table")
        return {}
    return loaded


def table_list(data: dict[str, Any], key: str, path: Path, errors: list[str]) -> list[Any]:
    value = data.get(key, [])
    if not isinstance(value, list):
        errors.append(f"{path.as_posix()}: {key!r} must be an array of tables")
        return []
    return value


def require_string(
    table: dict[str, Any], key: str, context: str, errors: list[str]
) -> str | None:
    value = table.get(key)
    if not isinstance(value, str) or not value:
        errors.append(f"{context}: {key} must be a non-empty string")
        return None
    return value


def require_bool(
    table: dict[str, Any], key: str, context: str, errors: list[str]
) -> bool | None:
    value = table.get(key)
    if not isinstance(value, bool):
        errors.append(f"{context}: {key} must be a boolean")
        return None
    return value


def require_positive_int(
    table: dict[str, Any], key: str, context: str, errors: list[str]
) -> int | None:
    value = table.get(key)
    if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
        errors.append(f"{context}: {key} must be a positive integer")
        return None
    return value


def require_string_list(
    table: dict[str, Any], key: str, context: str, errors: list[str]
) -> tuple[str, ...] | None:
    value = table.get(key)
    if not isinstance(value, list) or not value:
        errors.append(f"{context}: {key} must be a non-empty string array")
        return None
    items: list[str] = []
    for index, item in enumerate(value):
        if not isinstance(item, str) or not item:
            errors.append(f"{context}: {key}[{index}] must be a non-empty string")
            continue
        items.append(item)
    return tuple(items)


def validate_relative_posix_path(path: str, context: str, errors: list[str]) -> None:
    if "\\" in path:
        errors.append(f"{context}: path must use '/' separators: {path!r}")
    parts = PurePosixPath(path).parts
    if PurePosixPath(path).is_absolute():
        errors.append(f"{context}: path must be relative: {path!r}")
    if ".." in parts:
        errors.append(f"{context}: path must not traverse upward: {path!r}")


def is_under(path: str, prefix: str) -> bool:
    return path.startswith(prefix)


def build_targets(data: dict[str, Any], path: Path, errors: list[str]) -> tuple[TargetSpec, ...]:
    if data.get("schema_version") != TARGETS_SCHEMA_VERSION:
        errors.append(
            f"{path.as_posix()}: schema_version {data.get('schema_version')!r} "
            f"does not match {TARGETS_SCHEMA_VERSION!r}"
        )

    targets: list[TargetSpec] = []
    for index, raw in enumerate(table_list(data, "target", path, errors)):
        context = f"{path.as_posix()} [[target]] #{index + 1}"
        if not isinstance(raw, dict):
            errors.append(f"{context}: entry must be a table")
            continue
        name = require_string(raw, "name", context, errors)
        source_path = require_string(raw, "path", context, errors)
        corpus_dir = require_string(raw, "corpus_dir", context, errors)
        generator = require_string(raw, "generator", context, errors)
        max_input_bytes = require_positive_int(raw, "max_input_bytes", context, errors)
        invalid_input_policy = require_string(raw, "invalid_input_policy", context, errors)
        if (
            name is None
            or source_path is None
            or corpus_dir is None
            or generator is None
            or max_input_bytes is None
            or invalid_input_policy is None
        ):
            continue
        validate_relative_posix_path(source_path, f"{context} path", errors)
        validate_relative_posix_path(corpus_dir, f"{context} corpus_dir", errors)
        if not source_path.startswith(TARGET_PATH_PREFIX) or not source_path.endswith(".rs"):
            errors.append(
                f"{context}: path must stay under {TARGET_PATH_PREFIX}*.rs: "
                f"{source_path!r}"
            )
        if not is_under(corpus_dir, CORPUS_DIR_PREFIX):
            errors.append(
                f"{context}: corpus_dir must stay under {CORPUS_DIR_PREFIX}: "
                f"{corpus_dir!r}"
            )
        if generator != GENERATOR_PATH:
            errors.append(
                f"{context}: generator {generator!r} does not match {GENERATOR_PATH!r}"
            )
        targets.append(
            TargetSpec(
                name=name,
                path=source_path,
                corpus_dir=corpus_dir,
                generator=generator,
                max_input_bytes=max_input_bytes,
                invalid_input_policy=invalid_input_policy,
            )
        )
    if not targets:
        errors.append(f"{path.as_posix()}: at least one [[target]] entry is required")
    return tuple(targets)


def build_support(data: dict[str, Any], path: Path, errors: list[str]) -> tuple[SupportSpec, ...]:
    support: list[SupportSpec] = []
    for index, raw in enumerate(table_list(data, "support", path, errors)):
        context = f"{path.as_posix()} [[support]] #{index + 1}"
        if not isinstance(raw, dict):
            errors.append(f"{context}: entry must be a table")
            continue
        source_path = require_string(raw, "path", context, errors)
        used_by = require_string_list(raw, "used_by", context, errors)
        if source_path is None or used_by is None:
            continue
        validate_relative_posix_path(source_path, f"{context} path", errors)
        if not source_path.startswith(TARGET_PATH_PREFIX) or not source_path.endswith(".rs"):
            errors.append(
                f"{context}: path must stay under {TARGET_PATH_PREFIX}*.rs: "
                f"{source_path!r}"
            )
        support.append(SupportSpec(path=source_path, used_by=used_by))
    return tuple(support)


def build_bins(data: dict[str, Any], path: Path, errors: list[str]) -> tuple[CargoBinSpec, ...]:
    package = data.get("package", {})
    metadata = package.get("metadata", {}) if isinstance(package, dict) else {}
    if not isinstance(metadata, dict) or metadata.get("cargo-fuzz") is not True:
        errors.append(f"{path.as_posix()}: [package.metadata] cargo-fuzz must be true")

    bins: list[CargoBinSpec] = []
    for index, raw in enumerate(table_list(data, "bin", path, errors)):
        context = f"{path.as_posix()} [[bin]] #{index + 1}"
        if not isinstance(raw, dict):
            errors.append(f"{context}: entry must be a table")
            continue
        name = require_string(raw, "name", context, errors)
        source_path = require_string(raw, "path", context, errors)
        test = require_bool(raw, "test", context, errors)
        doc = require_bool(raw, "doc", context, errors)
        if name is None or source_path is None or test is None or doc is None:
            continue
        validate_relative_posix_path(source_path, f"{context} path", errors)
        if test is not False:
            errors.append(f"{context}: test must be false")
        if doc is not False:
            errors.append(f"{context}: doc must be false")
        bins.append(CargoBinSpec(name=name, path=source_path, test=test, doc=doc))
    return tuple(bins)


def build_manifest_entries(
    data: dict[str, Any], path: Path, errors: list[str]
) -> tuple[ManifestEntry, ...]:
    if data.get("schema_version") != CORPUS_SCHEMA_VERSION:
        errors.append(
            f"{path.as_posix()}: schema_version {data.get('schema_version')!r} "
            f"does not match {CORPUS_SCHEMA_VERSION!r}"
        )
    if data.get("generated_by") != GENERATOR_PATH:
        errors.append(
            f"{path.as_posix()}: generated_by {data.get('generated_by')!r} "
            f"does not match {GENERATOR_PATH!r}"
        )

    entries: list[ManifestEntry] = []
    for index, raw in enumerate(table_list(data, "entry", path, errors)):
        context = f"{path.as_posix()} [[entry]] #{index + 1}"
        if not isinstance(raw, dict):
            errors.append(f"{context}: entry must be a table")
            continue
        target = require_string(raw, "target", context, errors)
        corpus_dir = require_string(raw, "corpus_dir", context, errors)
        seed_files = require_string_list(raw, "seed_files", context, errors)
        generator = require_string(raw, "generator", context, errors)
        if target is None or corpus_dir is None or seed_files is None or generator is None:
            continue
        validate_relative_posix_path(corpus_dir, f"{context} corpus_dir", errors)
        if not is_under(corpus_dir, CORPUS_DIR_PREFIX):
            errors.append(
                f"{context}: corpus_dir must stay under {CORPUS_DIR_PREFIX}: "
                f"{corpus_dir!r}"
            )
        for seed_file in seed_files:
            if "/" in seed_file or "\\" in seed_file:
                errors.append(
                    f"{context}: seed_files entries must be basenames: {seed_file!r}"
                )
        entries.append(
            ManifestEntry(
                target=target,
                corpus_dir=corpus_dir,
                seed_files=seed_files,
                generator=generator,
            )
        )
    return tuple(entries)


def duplicate_values(values: list[str]) -> list[str]:
    seen: set[str] = set()
    duplicates: set[str] = set()
    for value in values:
        if value in seen:
            duplicates.add(value)
        seen.add(value)
    return sorted(duplicates)


def validate_registry(root: Path) -> tuple[Registry, list[str]]:
    errors: list[str] = []
    targets_path = root / TARGETS_REGISTRY_PATH
    cargo_path = root / "fuzz" / "Cargo.toml"
    manifest_path = root / CORPUS_MANIFEST_PATH

    targets_data = load_toml(targets_path, errors)
    cargo_data = load_toml(cargo_path, errors)
    manifest_data = load_toml(manifest_path, errors)

    targets = build_targets(targets_data, targets_path, errors)
    support = build_support(targets_data, targets_path, errors)
    bins = build_bins(cargo_data, cargo_path, errors)
    manifest_entries = build_manifest_entries(manifest_data, manifest_path, errors)
    registry = Registry(targets, support, bins, manifest_entries)

    validate_name_sets(root, registry, errors)
    validate_filesystem(root, registry, errors)
    validate_workflow(root, registry, errors)
    validate_git_tracked_corpus(root, registry, errors)
    validate_runtime_artifact_hygiene(root, errors)
    return registry, errors


def validate_name_sets(root: Path, registry: Registry, errors: list[str]) -> None:
    target_names = [target.name for target in registry.targets]
    target_paths = [target.path for target in registry.targets]
    support_paths = [item.path for item in registry.support]
    bin_names = [item.name for item in registry.bins]
    bin_paths = [item.path for item in registry.bins]
    manifest_targets = [entry.target for entry in registry.manifest_entries]

    for label, values in (
        ("tests/fuzzing/targets.toml target names", target_names),
        ("tests/fuzzing/targets.toml target paths", target_paths),
        ("tests/fuzzing/targets.toml support paths", support_paths),
        ("fuzz/Cargo.toml bin names", bin_names),
        ("fuzz/Cargo.toml bin paths", bin_paths),
        ("tests/fuzzing/corpus/manifest.toml targets", manifest_targets),
    ):
        duplicates = duplicate_values(values)
        if duplicates:
            errors.append(f"{label} contains duplicates: {duplicates}")

    target_name_set = set(target_names)
    target_path_set = set(target_paths)
    support_path_set = set(support_paths)
    bin_by_name = {item.name: item for item in registry.bins}
    manifest_by_target = {entry.target: entry for entry in registry.manifest_entries}

    if sorted(bin_names) != sorted(target_names):
        errors.append(
            "fuzz/Cargo.toml bin names do not match tests/fuzzing/targets.toml target names: "
            f"actual={sorted(bin_names)} expected={sorted(target_names)}"
        )
    if sorted(manifest_targets) != sorted(target_names):
        errors.append(
            "tests/fuzzing/corpus/manifest.toml targets do not match tests/fuzzing/targets.toml: "
            f"actual={sorted(manifest_targets)} expected={sorted(target_names)}"
        )

    overlapping_paths = sorted(target_path_set & support_path_set)
    if overlapping_paths:
        errors.append(f"files cannot be both fuzz targets and support files: {overlapping_paths}")

    support_bins = sorted(support_path_set & set(bin_paths))
    if support_bins:
        errors.append(f"support files must not be Cargo fuzz bins: {support_bins}")

    for target in registry.targets:
        cargo_bin = bin_by_name.get(target.name)
        if cargo_bin is not None and cargo_bin.path != target.path:
            errors.append(
                f"{target.name}: fuzz/Cargo.toml path {cargo_bin.path!r} "
                f"does not match tests/fuzzing/targets.toml path {target.path!r}"
            )
        entry = manifest_by_target.get(target.name)
        if entry is not None and entry.corpus_dir != target.corpus_dir:
            errors.append(
                f"{target.name}: manifest corpus_dir {entry.corpus_dir!r} "
                f"does not match tests/fuzzing/targets.toml corpus_dir {target.corpus_dir!r}"
            )
        if entry is not None and entry.generator != "deterministic-bytes-v1":
            errors.append(
                f"{target.name}: manifest generator {entry.generator!r} "
                "does not match 'deterministic-bytes-v1'"
            )

    for item in registry.support:
        unknown_targets = sorted(set(item.used_by) - target_name_set)
        if unknown_targets:
            errors.append(f"{item.path}: support used_by unknown targets {unknown_targets}")

    _ = root


def validate_filesystem(root: Path, registry: Registry, errors: list[str]) -> None:
    fuzz_root = root / "fuzz"
    sources_root = fuzz_root / "fuzz_targets"
    corpus_root = root / "tests" / "fuzzing" / "corpus"

    expected_source_paths = sorted(
        [target.path for target in registry.targets] + [item.path for item in registry.support]
    )
    actual_source_paths = sorted(
        path.relative_to(fuzz_root).as_posix()
        for path in sources_root.glob("*.rs")
        if path.is_file()
    )
    if actual_source_paths != expected_source_paths:
        errors.append(
            "fuzz/fuzz_targets source set does not match tests/fuzzing/targets.toml: "
            f"actual={actual_source_paths} expected={expected_source_paths}"
        )

    for target in registry.targets:
        source_path = fuzz_root / target.path
        if not source_path.is_file():
            errors.append(f"{target.name}: missing fuzz target source {source_path.as_posix()}")
            continue
        source = source_path.read_text(encoding="utf-8", errors="ignore")
        if "fuzz_target!" not in source:
            errors.append(f"{target.name}: target source does not contain fuzz_target!")

    for item in registry.support:
        support_path = fuzz_root / item.path
        if not support_path.is_file():
            errors.append(f"{item.path}: missing support source {support_path.as_posix()}")
            continue
        source = support_path.read_text(encoding="utf-8", errors="ignore")
        if "fuzz_target!" in source:
            errors.append(f"{item.path}: support source must not define fuzz_target!")

    expected_corpus_dirs = sorted({target.corpus_dir for target in registry.targets})
    actual_corpus_dirs = sorted(
        path.relative_to(root).as_posix() for path in corpus_root.iterdir() if path.is_dir()
    )
    if actual_corpus_dirs != expected_corpus_dirs:
        errors.append(
            "tests/fuzzing/corpus directory set does not match target corpus_dir values: "
            f"actual={actual_corpus_dirs} expected={expected_corpus_dirs}"
        )

    manifest_by_target = {entry.target: entry for entry in registry.manifest_entries}
    for target in registry.targets:
        corpus_dir = root / target.corpus_dir
        if not corpus_dir.is_dir():
            errors.append(f"{target.name}: missing corpus directory {target.corpus_dir}")
            continue
        entry = manifest_by_target.get(target.name)
        if entry is None:
            continue
        actual_seed_files = sorted(path.name for path in corpus_dir.iterdir() if path.is_file())
        expected_seed_files = sorted(entry.seed_files)
        if actual_seed_files != expected_seed_files:
            errors.append(
                f"{target.name}: corpus files do not match manifest seed_files: "
                f"actual={actual_seed_files} expected={expected_seed_files}"
            )
        for seed_file in entry.seed_files:
            seed_path = corpus_dir / seed_file
            if not seed_path.is_file():
                errors.append(f"{target.name}: missing corpus seed {seed_path.as_posix()}")


def validate_workflow(root: Path, registry: Registry, errors: list[str]) -> None:
    workflow_path = root / WORKFLOW_PATH
    if not workflow_path.is_file():
        errors.append(f"missing workflow: {WORKFLOW_PATH}")
        return
    workflow = workflow_path.read_text(encoding="utf-8")
    required_fragments = [
        "python fuzz/generators/generate_seed_corpus.py --check",
        "python tools/testing/fuzz_registry_check.py",
        "python tools/testing/fuzz_registry_check.py --list-targets",
        "while IFS=$'\\t' read -r target corpus_dir; do",
        CANONICAL_RUN_COMMAND,
    ]
    for fragment in required_fragments:
        if fragment not in workflow:
            errors.append(f"{WORKFLOW_PATH}: missing required command shape: {fragment}")

    for line_number, line in enumerate(workflow.splitlines(), start=1):
        if "cargo +nightly fuzz run" in line and CANONICAL_RUN_COMMAND not in line:
            errors.append(
                f"{WORKFLOW_PATH}:{line_number}: fuzz run command must use the "
                "registry target and corpus_dir shape"
            )
        if "find tests/fuzzing/corpus" in line and "-delete" in line:
            errors.append(
                f"{WORKFLOW_PATH}:{line_number}: workflow must not delete under tests/fuzzing/corpus"
            )

    forbidden_fragments = [
        "--ensure-only",
        "rm -rf tests/fuzzing/corpus",
        "git clean",
    ]
    for fragment in forbidden_fragments:
        if fragment in workflow:
            errors.append(f"{WORKFLOW_PATH}: forbidden corpus mutation fragment: {fragment}")

    _ = registry


def validate_git_tracked_corpus(root: Path, registry: Registry, errors: list[str]) -> None:
    if not (root / ".git").exists():
        return
    try:
        result = subprocess.run(
            ["git", "ls-files", "--", "tests/fuzzing/corpus"],
            cwd=root,
            check=False,
            capture_output=True,
            text=True,
        )
    except OSError:
        return
    if result.returncode != 0:
        return

    expected = {CORPUS_MANIFEST_PATH}
    for entry in registry.manifest_entries:
        for seed_file in entry.seed_files:
            expected.add(f"{entry.corpus_dir}/{seed_file}")

    tracked = {line.strip().replace("\\", "/") for line in result.stdout.splitlines() if line.strip()}
    missing = sorted(expected - tracked)
    extra = sorted(tracked - expected)
    if missing:
        errors.append(f"manifest corpus seeds are not tracked by git: {missing}")
    if extra:
        errors.append(f"tracked tests/fuzzing/corpus files are not listed in manifest: {extra}")


def is_allowed_runtime_output(path: Path, root: Path) -> bool:
    relative = path.relative_to(root).as_posix()
    if any(relative.startswith(prefix) for prefix in ALLOWED_RUNTIME_OUTPUT_PREFIXES):
        return True
    return any(relative.startswith(prefix) for prefix in RUNTIME_HYGIENE_EXEMPT_PATH_PREFIXES)


def validate_runtime_artifact_hygiene(root: Path, errors: list[str]) -> None:
    scan_roots = (
        root / "fuzz",
        root / "tests" / "fuzzing",
        root / "tools" / "testing",
    )
    seen: set[str] = set()
    for scan_root in scan_roots:
        if not scan_root.is_dir():
            continue
        for path in scan_root.rglob("*"):
            if is_allowed_runtime_output(path, root):
                continue
            relative = path.relative_to(root).as_posix()
            if path.is_dir() and path.name in FORBIDDEN_RUNTIME_DIR_NAMES:
                message = (
                    f"runtime artifact directory is not allowed in source tree: {relative}; "
                    "use fuzz/target or an explicit temporary directory"
                )
                if message not in seen:
                    seen.add(message)
                    errors.append(message)
                continue
            if not path.is_file():
                continue
            filename = path.name
            if filename.endswith(FORBIDDEN_RUNTIME_FILE_SUFFIXES):
                message = (
                    f"runtime artifact file is not allowed in source tree: {relative}; "
                    "use fuzz/target or an explicit temporary directory"
                )
                if message not in seen:
                    seen.add(message)
                    errors.append(message)
                continue
            if filename.startswith(FORBIDDEN_RUNTIME_FILE_PREFIXES):
                message = (
                    f"runtime fuzz artifact is not allowed in source tree: {relative}; "
                    "use fuzz/target or an explicit temporary directory"
                )
                if message not in seen:
                    seen.add(message)
                    errors.append(message)


def print_errors(errors: list[str]) -> None:
    for error in errors:
        print(f"ERROR: {error}", file=sys.stderr)


def default_root() -> Path:
    return Path(__file__).resolve().parents[2]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Read-only check for Andromeda fuzz registry and CI command shape"
    )
    parser.add_argument("--root", type=Path, default=default_root())
    parser.add_argument(
        "--list-targets",
        action="store_true",
        help="After validation, emit target and corpus_dir as tab-separated rows.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = args.root.resolve()
    registry, errors = validate_registry(root)
    if errors:
        print_errors(errors)
        return 1

    if args.list_targets:
        for target in registry.targets:
            print(f"{target.name}\t{target.corpus_dir}")
    else:
        seed_count = sum(len(entry.seed_files) for entry in registry.manifest_entries)
        corpus_count = len({target.corpus_dir for target in registry.targets})
        print(
            "fuzz registry check passed: "
            f"{len(registry.targets)} targets, "
            f"{len(registry.support)} support files, "
            f"{corpus_count} corpus directories, "
            f"{seed_count} deterministic seeds"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
