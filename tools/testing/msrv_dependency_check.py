#!/usr/bin/env python3
"""Read-only best-effort MSRV checker for Cargo.lock.

The checker uses only the Python standard library. It reads Cargo.lock and,
unless a baseline is provided, Cargo.toml. It never invokes Cargo, updates the
lockfile, writes evidence files, or uses network access.
"""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional, Sequence, Tuple


KEY_VALUE_RE = re.compile(r'^\s*([A-Za-z0-9_.-]+)\s*=\s*"([^"]*)"\s*(?:#.*)?$')
SECTION_RE = re.compile(r"^\s*\[([^\]]+)\]\s*(?:#.*)?$")
NUMERIC_VERSION_RE = re.compile(r"^\s*(\d+)(?:\.(\d+))?(?:\.(\d+))?")


@dataclass(frozen=True)
class LockedPackage:
    name: str
    version: str
    source: Optional[str]
    rust_version: Optional[str]


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Check package rust-version metadata recorded in Cargo.lock."
    )
    parser.add_argument(
        "--lockfile",
        default="Cargo.lock",
        help="Path to Cargo.lock. Defaults to %(default)s.",
    )
    parser.add_argument(
        "--manifest",
        default="Cargo.toml",
        help="Path to the root Cargo.toml used to infer the baseline.",
    )
    parser.add_argument(
        "--baseline",
        help="Rust baseline to compare against, for example 1.95.",
    )
    parser.add_argument(
        "--show-compatible",
        action="store_true",
        help="Also list packages whose available rust-version metadata is compatible.",
    )
    return parser.parse_args(argv)


def parse_numeric_version(value: str) -> Optional[Tuple[int, int, int]]:
    match = NUMERIC_VERSION_RE.match(value)
    if not match:
        return None

    parts = []
    for group in match.groups():
        parts.append(int(group) if group is not None else 0)
    return (parts[0], parts[1], parts[2])


def display_source(source: Optional[str]) -> str:
    if source is None:
        return "path"
    if source.startswith("registry+https://github.com/rust-lang/crates.io-index"):
        return "crates.io"
    return source


def package_from_table(
    table: Dict[str, str], line_number: int, warnings: List[str]
) -> Optional[LockedPackage]:
    name = table.get("name")
    version = table.get("version")
    if name is None or version is None:
        warnings.append(
            f"Skipping package table ending near line {line_number}: missing name or version."
        )
        return None

    rust_version = table.get("rust-version") or table.get("rust_version")
    return LockedPackage(
        name=name,
        version=version,
        source=table.get("source"),
        rust_version=rust_version,
    )


def parse_lockfile(lockfile: Path) -> Tuple[List[LockedPackage], List[str]]:
    warnings: List[str] = []
    packages: List[LockedPackage] = []
    current: Optional[Dict[str, str]] = None
    current_starts_at = 0

    with lockfile.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, start=1):
            stripped = line.strip()

            if stripped == "[[package]]":
                if current is not None:
                    package = package_from_table(current, line_number - 1, warnings)
                    if package is not None:
                        packages.append(package)
                current = {}
                current_starts_at = line_number
                continue

            if stripped.startswith("[[") and stripped.endswith("]]"):
                if current is not None:
                    package = package_from_table(current, line_number - 1, warnings)
                    if package is not None:
                        packages.append(package)
                current = None
                continue

            if current is None:
                continue

            match = KEY_VALUE_RE.match(line)
            if match is None:
                continue

            key, value = match.groups()
            if key in {"name", "version", "source", "rust-version", "rust_version"}:
                current[key] = value

    if current is not None:
        package = package_from_table(current, current_starts_at, warnings)
        if package is not None:
            packages.append(package)

    return packages, warnings


def parse_manifest_baseline(manifest: Path) -> Optional[str]:
    section = ""
    package_baseline: Optional[str] = None

    with manifest.open("r", encoding="utf-8") as handle:
        for line in handle:
            section_match = SECTION_RE.match(line)
            if section_match is not None:
                section = section_match.group(1).strip()
                continue

            key_value_match = KEY_VALUE_RE.match(line)
            if key_value_match is None:
                continue

            key, value = key_value_match.groups()
            if key != "rust-version":
                continue

            if section == "workspace.package":
                return value
            if section == "package" and package_baseline is None:
                package_baseline = value

    return package_baseline


def print_package_table(title: str, packages: Sequence[LockedPackage]) -> None:
    if not packages:
        return

    print(title)
    print("package\tversion\trust-version\tsource")
    for package in packages:
        print(
            f"{package.name}\t{package.version}\t"
            f"{package.rust_version}\t{display_source(package.source)}"
        )


def main(argv: Sequence[str]) -> int:
    args = parse_args(argv)
    lockfile = Path(args.lockfile)
    manifest = Path(args.manifest)

    if not lockfile.is_file():
        print(f"error: lockfile not found: {lockfile}", file=sys.stderr)
        return 2

    baseline = args.baseline
    if baseline is None:
        if not manifest.is_file():
            print(
                "error: --baseline was not provided and manifest was not found: "
                f"{manifest}",
                file=sys.stderr,
            )
            return 2
        baseline = parse_manifest_baseline(manifest)

    if baseline is None:
        print(
            "error: could not infer rust-version baseline; pass --baseline.",
            file=sys.stderr,
        )
        return 2

    baseline_version = parse_numeric_version(baseline)
    if baseline_version is None:
        print(f"error: could not parse baseline rust-version: {baseline}", file=sys.stderr)
        return 2

    packages, warnings = parse_lockfile(lockfile)
    with_metadata = [package for package in packages if package.rust_version is not None]
    incompatible: List[LockedPackage] = []
    compatible: List[LockedPackage] = []
    unparseable: List[LockedPackage] = []

    for package in with_metadata:
        parsed = parse_numeric_version(package.rust_version or "")
        if parsed is None:
            unparseable.append(package)
        elif parsed > baseline_version:
            incompatible.append(package)
        else:
            compatible.append(package)

    print("MSRV dependency check")
    print(f"baseline_rust_version={baseline}")
    print(f"lockfile={lockfile}")
    print(f"packages_scanned={len(packages)}")
    print(f"packages_with_rust_version={len(with_metadata)}")

    if warnings:
        print("warnings:")
        for warning in warnings:
            print(f"- {warning}")

    if unparseable:
        print("result=error")
        print_package_table("Packages with unparseable rust-version metadata:", unparseable)
        return 2

    if incompatible:
        print("result=failed")
        print_package_table(
            "Packages requiring a newer rustc than the baseline:", incompatible
        )
        if args.show_compatible:
            print_package_table("Compatible packages with rust-version metadata:", compatible)
        return 1

    if not with_metadata:
        print("result=inconclusive")
        print(
            "Cargo.lock contains no package rust-version metadata; no dependency "
            "MSRV conflicts can be detected from the lockfile alone."
        )
        return 0

    print("result=passed")
    print("No package rust-version metadata above the baseline was detected.")
    if args.show_compatible:
        print_package_table("Compatible packages with rust-version metadata:", compatible)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
