#!/usr/bin/env python3
"""Print or run the bounded Andromeda Miri subset."""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
NIGHTLY_TOOLCHAIN = "nightly"


@dataclass(frozen=True)
class MiriTarget:
    package: str
    path: str
    focus: str
    command: tuple[str, ...]


MIRI_TARGETS: tuple[MiriTarget, ...] = (
    MiriTarget(
        package="andromeda-maps",
        path="crates/andromeda-maps",
        focus="map descriptor and publication value invariants without GPU work",
        command=(
            "cargo",
            f"+{NIGHTLY_TOOLCHAIN}",
            "miri",
            "test",
            "-p",
            "andromeda-maps",
            "--lib",
            "--all-features",
            "--locked",
        ),
    ),
    MiriTarget(
        package="andromeda-policy",
        path="crates/andromeda-policy",
        focus="admission policy value handling and fail-closed checks",
        command=(
            "cargo",
            f"+{NIGHTLY_TOOLCHAIN}",
            "miri",
            "test",
            "-p",
            "andromeda-policy",
            "--lib",
            "--all-features",
            "--locked",
        ),
    ),
    MiriTarget(
        package="andromeda-resource",
        path="crates/andromeda-resource",
        focus="resource limits, checked arithmetic, and error construction",
        command=(
            "cargo",
            f"+{NIGHTLY_TOOLCHAIN}",
            "miri",
            "test",
            "-p",
            "andromeda-resource",
            "--lib",
            "--all-features",
            "--locked",
        ),
    ),
    MiriTarget(
        package="andromeda-procedure-store",
        path="crates/andromeda-procedure-store",
        focus="procedure evidence, status, identity, and sink value paths",
        command=(
            "cargo",
            f"+{NIGHTLY_TOOLCHAIN}",
            "miri",
            "test",
            "-p",
            "andromeda-procedure-store",
            "--lib",
            "--all-features",
            "--locked",
        ),
    ),
    MiriTarget(
        package="andromeda-contract",
        path="crates/andromeda-contract",
        focus="ProcedureContract shape, compatibility, and hash input handling",
        command=(
            "cargo",
            f"+{NIGHTLY_TOOLCHAIN}",
            "miri",
            "test",
            "-p",
            "andromeda-contract",
            "--lib",
            "--all-features",
            "--locked",
        ),
    ),
)


DEFAULT_EXCLUSIONS = (
    (
        "andromeda-proto",
        "has a build.rs and generated Protobuf surface; keep outside this light subset",
    ),
    (
        "andromeda-quic",
        "may pull async transport and TLS dependencies; validate with protocol-specific gates",
    ),
    (
        "andromeda-storage",
        "durability and page/WAL behavior require crash, fuzz, and owner recovery gates",
    ),
    (
        "andromeda-wal",
        "durable WAL promotion requires owner WAL tests and crash/recovery evidence",
    ),
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Print the bounded Miri subset for memory and codec-sensitive "
            "Andromeda crates. The default is a dry run."
        ),
    )
    parser.add_argument(
        "--run",
        action="store_true",
        help="Run the listed Miri commands sequentially. Default: print only.",
    )
    parser.add_argument(
        "--only",
        action="append",
        metavar="PACKAGE",
        help="Limit output or execution to one package. May be used more than once.",
    )
    parser.add_argument(
        "--check-env",
        action="store_true",
        help="Check whether cargo +nightly miri is available before printing.",
    )
    parser.add_argument(
        "--show-exclusions",
        action="store_true",
        help="Also print crates intentionally excluded from this light subset.",
    )
    return parser.parse_args()


def format_command(command: tuple[str, ...]) -> str:
    return " ".join(quote_arg(part) for part in command)


def quote_arg(part: str) -> str:
    if not part:
        return '""'
    if any(char.isspace() for char in part):
        return '"' + part.replace('"', '\\"') + '"'
    return part


def target_status(target: MiriTarget) -> str:
    crate_path = ROOT / target.path
    manifest_path = crate_path / "Cargo.toml"
    build_script_path = crate_path / "build.rs"
    if not crate_path.exists():
        return f"blocked: missing crate path {target.path}"
    if not manifest_path.exists():
        return f"blocked: missing manifest {target.path}/Cargo.toml"
    if build_script_path.exists():
        return f"blocked: build script present at {target.path}/build.rs"
    if manifest_declares_build_script(manifest_path):
        return f"blocked: build script declared in {target.path}/Cargo.toml"
    return "ready"


def manifest_declares_build_script(manifest_path: Path) -> bool:
    try:
        text = manifest_path.read_text(encoding="utf-8")
    except OSError:
        return False
    for line in text.splitlines():
        stripped = line.split("#", 1)[0].strip()
        if stripped.startswith("build") and "=" in stripped:
            return True
    return False


def selected_targets(only: list[str] | None) -> tuple[MiriTarget, ...]:
    if not only:
        return MIRI_TARGETS
    requested = set(only)
    return tuple(target for target in MIRI_TARGETS if target.package in requested)


def unknown_targets(only: list[str] | None) -> list[str]:
    if not only:
        return []
    known = {target.package for target in MIRI_TARGETS}
    return sorted(set(only) - known)


def check_miri_available() -> tuple[bool, str]:
    if shutil.which("cargo") is None:
        return False, "cargo is not available on PATH"
    try:
        result = subprocess.run(
            ["cargo", f"+{NIGHTLY_TOOLCHAIN}", "miri", "--version"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
    except OSError as exc:
        return False, f"cargo +{NIGHTLY_TOOLCHAIN} miri --version failed: {exc}"
    if result.returncode != 0:
        message = result.stderr.strip() or result.stdout.strip() or "unknown Miri error"
        return False, message
    return True, result.stdout.strip()


def print_targets(targets: tuple[MiriTarget, ...]) -> None:
    print("Andromeda Miri Subset")
    print(f"Repository: {ROOT}")
    print("Mode: dry-run")
    print()
    for index, target in enumerate(targets, start=1):
        print(f"{index}. {target.package}")
        print(f"   status: {target_status(target)}")
        print(f"   focus: {target.focus}")
        print(f"   command: {format_command(target.command)}")
    print()
    print("Dry run only. Add --run to execute these commands sequentially.")


def print_exclusions() -> None:
    print()
    print("Default Exclusions")
    for package, reason in DEFAULT_EXCLUSIONS:
        print(f"  {package}: {reason}")


def run_targets(targets: tuple[MiriTarget, ...]) -> int:
    available, message = check_miri_available()
    if not available:
        print("Miri unavailable")
        print(f"  {message}")
        return 2

    print("Miri available")
    print(f"  {message}")
    print()

    for target in targets:
        status = target_status(target)
        if status != "ready":
            print(f"Blocked: {target.package}")
            print(f"  {status}")
            return 2
        print(f"Running: {target.package}")
        print(f"  {format_command(target.command)}")
        result = subprocess.run(target.command, cwd=ROOT, check=False)
        if result.returncode != 0:
            print(f"Failed: {target.package} exited with {result.returncode}")
            return result.returncode
    return 0


def main() -> int:
    args = parse_args()
    unknown = unknown_targets(args.only)
    if unknown:
        print("Unknown package requested")
        for package in unknown:
            print(f"  {package}")
        return 2

    targets = selected_targets(args.only)
    if not targets:
        print("No Miri targets selected")
        return 2

    if args.check_env and not args.run:
        available, message = check_miri_available()
        status = "available" if available else "blocked"
        print(f"Miri environment: {status}")
        print(f"  {message}")
        print()

    if args.run:
        return run_targets(targets)

    print_targets(targets)
    if args.show_exclusions:
        print_exclusions()
    return 0


if __name__ == "__main__":
    sys.exit(main())
