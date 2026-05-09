#!/usr/bin/env python3
"""Read-only local preflight for Andromeda testing and devex gates."""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


@dataclass(frozen=True)
class ToolSpec:
    name: str
    executables: tuple[str, ...]
    required: bool
    purpose: str


@dataclass(frozen=True)
class ToolResult:
    spec: ToolSpec
    found_path: str | None


TOOL_SPECS = (
    ToolSpec("Python", tuple(), True, "runs repository validation scripts"),
    ToolSpec("git", ("git",), True, "summarizes worktree status"),
    ToolSpec("cargo", ("cargo",), True, "runs Rust workspace gates"),
    ToolSpec("rustc", ("rustc",), True, "compiles Rust crates"),
    ToolSpec("rustfmt", ("rustfmt",), True, "checks Rust formatting"),
    ToolSpec("clippy", ("cargo-clippy", "clippy-driver"), True, "runs Rust lint gates"),
    ToolSpec("rustup", ("rustup",), False, "manages optional nightly and Miri toolchains"),
    ToolSpec("cargo-nextest", ("cargo-nextest",), False, "runs the preferred workspace test gate"),
    ToolSpec("cargo-audit", ("cargo-audit",), False, "checks RustSec advisories"),
    ToolSpec("cargo-deny", ("cargo-deny",), False, "checks dependency policy"),
    ToolSpec("cargo-fuzz", ("cargo-fuzz",), False, "runs libFuzzer targets"),
    ToolSpec("cargo-miri", ("cargo-miri",), False, "runs Miri UB checks on nightly"),
    ToolSpec("cargo-vet", ("cargo-vet",), False, "checks supply-chain attestations when configured"),
)


CONFLICT_CODES = {"DD", "AU", "UD", "UA", "DU", "AA", "UU"}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Read-only Andromeda testing preflight.",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit with status 1 when required tools are missing.",
    )
    return parser.parse_args()


def rel(path: Path) -> str:
    try:
        return path.relative_to(ROOT).as_posix()
    except ValueError:
        return str(path)


def run_git_status() -> tuple[dict[str, int], list[str], str | None]:
    categories = {
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
        return categories, [], "git is not available on PATH"

    try:
        result = subprocess.run(
            ["git", "status", "--porcelain=v1"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
    except OSError as exc:
        return categories, [], f"git status failed: {exc}"

    if result.returncode != 0:
        stderr = result.stderr.strip() or "unknown git status error"
        return categories, [], stderr

    samples: list[str] = []
    for line in result.stdout.splitlines():
        if not line:
            continue
        code = line[:2]
        path = line[3:] if len(line) > 3 else ""
        categories["paths"] += 1
        if code == "??":
            categories["untracked"] += 1
        elif code == "!!":
            categories["ignored"] += 1
        else:
            index_status, worktree_status = code[0], code[1]
            if index_status != " ":
                categories["staged"] += 1
            if worktree_status != " ":
                categories["unstaged"] += 1
            if "M" in code:
                categories["modified"] += 1
            if "A" in code:
                categories["added"] += 1
            if "D" in code:
                categories["deleted"] += 1
            if "R" in code:
                categories["renamed"] += 1
            if "C" in code:
                categories["copied"] += 1
            if "U" in code or code in CONFLICT_CODES:
                categories["conflicted"] += 1
        if len(samples) < 12:
            samples.append(f"{code} {path}")

    return categories, samples, None


def check_tools() -> list[ToolResult]:
    results: list[ToolResult] = []
    for spec in TOOL_SPECS:
        if spec.name == "Python":
            python_path = str(Path(sys.executable)) if sys.executable else None
            results.append(ToolResult(spec, python_path))
            continue

        found_path = None
        for executable in spec.executables:
            found_path = shutil.which(executable)
            if found_path is not None:
                break
        results.append(ToolResult(spec, found_path))
    return results


def print_git_status() -> None:
    categories, samples, error = run_git_status()
    print("Git Status")
    if error is not None:
        print(f"  status: unavailable ({error})")
        return
    if categories["paths"] == 0:
        print("  status: clean")
        return
    for name in (
        "paths",
        "staged",
        "unstaged",
        "untracked",
        "modified",
        "added",
        "deleted",
        "renamed",
        "copied",
        "conflicted",
    ):
        print(f"  {name}: {categories[name]}")
    if samples:
        print("  sample:")
        for sample in samples:
            print(f"    {sample}")


def print_tools(results: list[ToolResult]) -> None:
    print("Tool Availability")
    for required in (True, False):
        label = "required" if required else "optional"
        print(f"  {label}:")
        for result in results:
            if result.spec.required != required:
                continue
            status = "found" if result.found_path else "missing"
            location = f" - {result.found_path}" if result.found_path else ""
            print(f"    {result.spec.name}: {status}{location}")
            print(f"      purpose: {result.spec.purpose}")


def main() -> int:
    args = parse_args()
    print("Andromeda Testing Preflight")
    print(f"Repository: {ROOT}")
    print(f"Python version: {sys.version.split()[0]}")
    print()

    print_git_status()
    print()

    tool_results = check_tools()
    print_tools(tool_results)
    print()

    missing_required = [
        result.spec.name
        for result in tool_results
        if result.spec.required and result.found_path is None
    ]
    if missing_required:
        print("Missing Required Tools")
        for name in missing_required:
            print(f"  {name}")
        return 1 if args.strict else 0

    print("Result: preflight completed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
