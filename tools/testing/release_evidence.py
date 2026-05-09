#!/usr/bin/env python3
"""Read-only local release evidence generator for Andromeda."""

from __future__ import annotations

import argparse
import json
import platform
import re
import shutil
import subprocess
import sys
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Mapping, Sequence


DEFAULT_ROOT = Path(__file__).resolve().parents[2]
SCHEMA = "andromeda.release_evidence.v1"
GENERATOR_VERSION = "0.1.0"
STATUS_ALIASES = {
    "pass": "pass",
    "passed": "pass",
    "fail": "fail",
    "failed": "fail",
    "skip": "skipped",
    "skipped": "skipped",
    "gap": "gap",
    "gaps": "gap",
    "partial": "partial",
}
STATUS_ORDER = ("pass", "fail", "skipped", "gap", "partial")
MAX_STATUS_SAMPLE = 25
COMMAND_TIMEOUT_SECONDS = 10


@dataclass(frozen=True)
class CommandResult:
    name: str
    command: str
    available: bool
    exit_code: int | None
    stdout: str
    stderr: str
    timed_out: bool


@dataclass(frozen=True)
class GitMetadata:
    available: bool
    root: str | None
    branch: str | None
    commit: str | None
    short_commit: str | None
    describe: str | None
    dirty: bool | None
    status_counts: dict[str, int]
    status_sample: tuple[str, ...]
    status_sample_truncated: bool
    error: str | None


@dataclass(frozen=True)
class EvidenceCheck:
    check_id: str
    status: str
    command: str
    source: str
    category: str
    artifact_path: str
    residual_risk: str
    follow_up: str
    notes: str


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Generate read-only local release evidence metadata. The script "
            "does not run validation gates and does not claim release readiness."
        ),
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
        help="Emit JSON instead of text.",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit with status 1 when supplied checks include fail or gap results.",
    )
    parser.add_argument(
        "--no-auto-detect",
        action="store_true",
        help="Do not add skipped records for detected local testing scripts.",
    )
    parser.add_argument(
        "--check",
        action="append",
        default=None,
        metavar="KEY=VALUE;...",
        help=(
            "Add a check record. Required key: status. Optional keys: id, "
            "command, source, category, artifact, residual_risk, follow_up, notes."
        ),
    )
    parser.add_argument(
        "--check-json",
        action="append",
        default=None,
        metavar="JSON",
        help="Add one check object or an array of check objects as JSON.",
    )
    parser.add_argument(
        "--checks-file",
        type=Path,
        action="append",
        default=None,
        help="Read check objects from a JSON file without modifying it.",
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


def command_to_text(command: Sequence[str]) -> str:
    return " ".join(command)


def run_metadata_command(
    root: Path,
    name: str,
    command: Sequence[str],
    timeout_seconds: int = COMMAND_TIMEOUT_SECONDS,
) -> CommandResult:
    executable = command[0]
    if shutil.which(executable) is None:
        return CommandResult(
            name=name,
            command=command_to_text(command),
            available=False,
            exit_code=None,
            stdout="",
            stderr=f"{executable} is not available on PATH",
            timed_out=False,
        )

    try:
        result = subprocess.run(
            list(command),
            cwd=root,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=timeout_seconds,
        )
    except subprocess.TimeoutExpired:
        return CommandResult(
            name=name,
            command=command_to_text(command),
            available=True,
            exit_code=None,
            stdout="",
            stderr=f"metadata command timed out after {timeout_seconds} seconds",
            timed_out=True,
        )
    except OSError as exc:
        return CommandResult(
            name=name,
            command=command_to_text(command),
            available=True,
            exit_code=None,
            stdout="",
            stderr=str(exc),
            timed_out=False,
        )

    return CommandResult(
        name=name,
        command=command_to_text(command),
        available=True,
        exit_code=result.returncode,
        stdout=result.stdout.strip(),
        stderr=result.stderr.strip(),
        timed_out=False,
    )


def first_line(value: str) -> str:
    for line in value.splitlines():
        stripped = line.strip()
        if stripped:
            return stripped
    return ""


def git_output(root: Path, command: Sequence[str]) -> tuple[str | None, str | None]:
    result = run_metadata_command(root, "git", command)
    if result.exit_code != 0:
        return None, result.stderr or f"{result.command} failed"
    return result.stdout, None


def parse_status_counts(status_output: str) -> tuple[dict[str, int], tuple[str, ...], bool]:
    counts = {
        "paths": 0,
        "staged": 0,
        "unstaged": 0,
        "untracked": 0,
        "ignored": 0,
        "modified": 0,
        "added": 0,
        "deleted": 0,
        "renamed": 0,
        "copied": 0,
        "conflicted": 0,
    }
    sample: list[str] = []

    for line in status_output.splitlines():
        if not line or line.startswith("##"):
            continue
        code = line[:2]
        path = line[3:] if len(line) > 3 else ""
        counts["paths"] += 1
        if code == "??":
            counts["untracked"] += 1
        elif code == "!!":
            counts["ignored"] += 1
        else:
            index_status = code[0]
            worktree_status = code[1]
            if index_status != " ":
                counts["staged"] += 1
            if worktree_status != " ":
                counts["unstaged"] += 1
            if "M" in code:
                counts["modified"] += 1
            if "A" in code:
                counts["added"] += 1
            if "D" in code:
                counts["deleted"] += 1
            if "R" in code:
                counts["renamed"] += 1
            if "C" in code:
                counts["copied"] += 1
            if "U" in code or code in {"DD", "AU", "UD", "UA", "DU", "AA", "UU"}:
                counts["conflicted"] += 1

        if len(sample) < MAX_STATUS_SAMPLE:
            sample.append(f"{code} {path}")

    return counts, tuple(sample), counts["paths"] > len(sample)


def collect_git_metadata(root: Path) -> GitMetadata:
    empty_counts = {
        "paths": 0,
        "staged": 0,
        "unstaged": 0,
        "untracked": 0,
        "ignored": 0,
        "modified": 0,
        "added": 0,
        "deleted": 0,
        "renamed": 0,
        "copied": 0,
        "conflicted": 0,
    }
    if shutil.which("git") is None:
        return GitMetadata(
            available=False,
            root=None,
            branch=None,
            commit=None,
            short_commit=None,
            describe=None,
            dirty=None,
            status_counts=empty_counts,
            status_sample=(),
            status_sample_truncated=False,
            error="git is not available on PATH",
        )

    repo_root, repo_error = git_output(root, ("git", "rev-parse", "--show-toplevel"))
    commit, commit_error = git_output(root, ("git", "rev-parse", "HEAD"))
    branch, branch_error = git_output(root, ("git", "rev-parse", "--abbrev-ref", "HEAD"))
    describe, _ = git_output(root, ("git", "describe", "--tags", "--always", "--dirty"))
    status, status_error = git_output(root, ("git", "status", "--porcelain=v1", "--branch"))

    errors = [
        error
        for error in (repo_error, commit_error, branch_error, status_error)
        if error is not None
    ]
    if status is None:
        counts, sample, truncated = empty_counts, (), False
    else:
        counts, sample, truncated = parse_status_counts(status)

    return GitMetadata(
        available=True,
        root=repo_root,
        branch=branch,
        commit=commit,
        short_commit=commit[:12] if commit else None,
        describe=describe,
        dirty=counts["paths"] > 0 if status is not None else None,
        status_counts=counts,
        status_sample=sample,
        status_sample_truncated=truncated,
        error="; ".join(errors) if errors else None,
    )


def collect_toolchain(root: Path) -> list[CommandResult]:
    commands = (
        ("python", (sys.executable, "--version") if sys.executable else ("python", "--version")),
        ("rustc", ("rustc", "-Vv")),
        ("cargo", ("cargo", "-V")),
        ("rustfmt", ("rustfmt", "--version")),
        ("clippy", ("cargo-clippy", "--version")),
        ("cargo-nextest", ("cargo-nextest", "--version")),
        ("cargo-audit", ("cargo-audit", "--version")),
        ("cargo-deny", ("cargo-deny", "--version")),
        ("cargo-fuzz", ("cargo-fuzz", "--version")),
        ("cargo-miri", ("cargo-miri", "--version")),
        ("rustup-active-toolchain", ("rustup", "show", "active-toolchain")),
    )
    return [run_metadata_command(root, name, command) for name, command in commands]


def normalize_status(value: object) -> str:
    status = str(value).strip().lower()
    normalized = STATUS_ALIASES.get(status)
    if normalized is None:
        allowed = ", ".join(STATUS_ORDER)
        raise argparse.ArgumentTypeError(f"unsupported check status {value!r}; use {allowed}")
    return normalized


def normalize_field_name(name: str) -> str:
    return name.strip().lower().replace("-", "_")


def parse_key_value_check(raw: str, index: int) -> EvidenceCheck:
    fields: dict[str, str] = {}
    for chunk in raw.split(";"):
        if not chunk.strip():
            continue
        if "=" not in chunk:
            raise argparse.ArgumentTypeError(
                f"invalid --check item {chunk!r}; expected key=value",
            )
        key, value = chunk.split("=", 1)
        fields[normalize_field_name(key)] = value.strip()

    if "status" not in fields:
        raise argparse.ArgumentTypeError("--check requires status=<pass|fail|skipped|gap|partial>")

    return check_from_mapping(fields, f"argument:{index}", index)


def check_from_mapping(
    raw: Mapping[str, Any],
    default_source: str,
    index: int,
) -> EvidenceCheck:
    fields = {normalize_field_name(str(key)): value for key, value in raw.items()}
    if "status" not in fields:
        raise argparse.ArgumentTypeError("check object requires a status field")

    artifact_path = fields.get("artifact_path", fields.get("artifact", ""))
    residual_risk = fields.get("residual_risk", fields.get("risk", ""))
    check_id = fields.get("check_id", fields.get("id", f"manual-{index:03d}"))
    source = fields.get("source", default_source)
    category = fields.get("category", "manual")

    return EvidenceCheck(
        check_id=str(check_id).strip(),
        status=normalize_status(fields["status"]),
        command=str(fields.get("command", "")).strip(),
        source=str(source).strip(),
        category=str(category).strip(),
        artifact_path=str(artifact_path).strip(),
        residual_risk=str(residual_risk).strip(),
        follow_up=str(fields.get("follow_up", "")).strip(),
        notes=str(fields.get("notes", "")).strip(),
    )


def load_json_checks(raw_json: str, default_source: str, start_index: int) -> list[EvidenceCheck]:
    try:
        parsed = json.loads(raw_json)
    except json.JSONDecodeError as exc:
        raise argparse.ArgumentTypeError(f"invalid check JSON: {exc}") from exc

    objects = parsed if isinstance(parsed, list) else [parsed]
    checks: list[EvidenceCheck] = []
    for offset, item in enumerate(objects):
        if not isinstance(item, Mapping):
            raise argparse.ArgumentTypeError("check JSON must be an object or array of objects")
        checks.append(check_from_mapping(item, default_source, start_index + offset))
    return checks


def load_checks_file(path: Path, start_index: int) -> list[EvidenceCheck]:
    try:
        raw_json = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise argparse.ArgumentTypeError(f"cannot read checks file {path}: {exc}") from exc
    return load_json_checks(raw_json, f"file:{path.as_posix()}", start_index)


def collect_supplied_checks(args: argparse.Namespace) -> list[EvidenceCheck]:
    checks: list[EvidenceCheck] = []
    for raw in args.check or ():
        checks.append(parse_key_value_check(raw, len(checks) + 1))

    for raw_json in args.check_json or ():
        checks.extend(load_json_checks(raw_json, "argument:json", len(checks) + 1))

    for path in args.checks_file or ():
        checks.extend(load_checks_file(path, len(checks) + 1))

    return checks


def python_command_for_script(root: Path, script: Path) -> str:
    relative = rel(root, script)
    text = read_text(script)
    command = f"python -B {relative}"
    if re.search(r'add_argument\(\s*["\']--json["\']', text):
        return f"{command} --json"
    if "--format" in text and "json" in text:
        return f"{command} --format json"
    return command


def auto_detect_checks(root: Path) -> list[EvidenceCheck]:
    scripts_dir = root / "tools" / "testing"
    if not scripts_dir.exists():
        return []

    checks: list[EvidenceCheck] = []
    for script in sorted(scripts_dir.glob("*.py")):
        if script.name == Path(__file__).name:
            continue
        check_id = f"local-tooling.{script.stem.replace('_', '-')}"
        checks.append(
            EvidenceCheck(
                check_id=check_id,
                status="skipped",
                command=python_command_for_script(root, script),
                source=rel(root, script),
                category="auto-detected-local-tooling",
                artifact_path="",
                residual_risk=(
                    "Auto-detected helper was not executed by release_evidence.py. "
                    "Run the command separately and pass the result with --check "
                    "when it is in release scope."
                ),
                follow_up="",
                notes="Detected from tools/testing without executing the script.",
            )
        )
    return checks


def summarize_checks(checks: Sequence[EvidenceCheck]) -> dict[str, object]:
    counts = {status: 0 for status in STATUS_ORDER}
    for check in checks:
        counts[check.status] += 1

    return {
        "total": len(checks),
        "pass": counts["pass"],
        "fail": counts["fail"],
        "skipped": counts["skipped"],
        "gap": counts["gap"],
        "partial": counts["partial"],
        "has_failed_checks": counts["fail"] > 0,
        "has_release_gaps": counts["gap"] > 0,
        "release_readiness_claim": False,
    }


def build_report(args: argparse.Namespace) -> dict[str, object]:
    root = args.root.resolve()
    supplied_checks = collect_supplied_checks(args)
    detected_checks = [] if args.no_auto_detect else auto_detect_checks(root)
    checks = [*supplied_checks, *detected_checks]
    git = collect_git_metadata(root)
    toolchain = collect_toolchain(root)

    return {
        "schema": SCHEMA,
        "generated_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "generator": {
            "name": "tools/testing/release_evidence.py",
            "version": GENERATOR_VERSION,
            "read_only": True,
            "writes_files": False,
            "runs_validation_gates": False,
            "release_readiness_claim": False,
            "safety_note": (
                "This report captures local metadata and declared check results. "
                "It does not execute release gates and does not approve a release."
            ),
        },
        "environment": {
            "repository_root": root.as_posix(),
            "cwd": Path.cwd().as_posix(),
            "platform": platform.platform(),
            "python_executable": sys.executable,
            "python_version": platform.python_version(),
        },
        "git": asdict(git),
        "toolchain": [asdict(entry) for entry in toolchain],
        "checks": [asdict(check) for check in checks],
        "summary": summarize_checks(checks),
    }


def print_text(report: Mapping[str, object]) -> None:
    summary = report["summary"]
    git = report["git"]
    assert isinstance(summary, Mapping)
    assert isinstance(git, Mapping)

    print("Andromeda Local Release Evidence")
    print(f"Schema: {report['schema']}")
    print(f"Generated UTC: {report['generated_at_utc']}")
    print("Release readiness claim: false")
    print()

    print("Repository")
    print(f"  root: {git.get('root') or report['environment']['repository_root']}")
    print(f"  branch: {git.get('branch') or 'unknown'}")
    print(f"  commit: {git.get('commit') or 'unknown'}")
    print(f"  describe: {git.get('describe') or 'unknown'}")
    status_counts = git.get("status_counts") or {}
    if isinstance(status_counts, Mapping):
        print(f"  dirty paths: {status_counts.get('paths', 0)}")
        print(f"  staged: {status_counts.get('staged', 0)}")
        print(f"  unstaged: {status_counts.get('unstaged', 0)}")
        print(f"  untracked: {status_counts.get('untracked', 0)}")
    if git.get("error"):
        print(f"  git metadata warning: {git['error']}")
    print()

    print("Toolchain")
    for raw_entry in report["toolchain"]:
        assert isinstance(raw_entry, Mapping)
        status = "found" if raw_entry["available"] else "missing"
        output = first_line(str(raw_entry.get("stdout") or raw_entry.get("stderr") or ""))
        suffix = f" - {output}" if output else ""
        print(f"  {raw_entry['name']}: {status}{suffix}")
    print()

    print("Checks")
    checks = report["checks"]
    assert isinstance(checks, list)
    if not checks:
        print("  none")
    for raw_check in checks:
        assert isinstance(raw_check, Mapping)
        print(f"  [{raw_check['status']}] {raw_check['check_id']}")
        if raw_check.get("command"):
            print(f"    command: {raw_check['command']}")
        print(f"    source: {raw_check['source']}")
        if raw_check.get("artifact_path"):
            print(f"    artifact: {raw_check['artifact_path']}")
        if raw_check.get("residual_risk"):
            print(f"    residual risk: {raw_check['residual_risk']}")
    print()

    print("Summary")
    print(f"  total: {summary['total']}")
    print(f"  pass: {summary['pass']}")
    print(f"  fail: {summary['fail']}")
    print(f"  skipped: {summary['skipped']}")
    print(f"  gap: {summary['gap']}")
    print(f"  partial: {summary['partial']}")
    print("  release readiness claim: false")


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        report = build_report(args)
    except argparse.ArgumentTypeError as exc:
        print(f"release_evidence.py: error: {exc}", file=sys.stderr)
        return 2

    if args.json:
        json.dump(report, sys.stdout, indent=2, sort_keys=True)
        print()
    else:
        print_text(report)

    summary = report["summary"]
    assert isinstance(summary, Mapping)
    if args.strict and (summary["fail"] or summary["gap"]):
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
