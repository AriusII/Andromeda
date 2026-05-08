#!/usr/bin/env python3
"""Read-only best-effort MSRV checker for Cargo.lock or Cargo metadata.

The default checker reads Cargo.lock and, unless a baseline is provided,
Cargo.toml. The optional --cargo-metadata mode invokes cargo metadata in locked
mode and reads package rust_version fields from Cargo's resolver output.
Neither mode updates the lockfile, writes evidence files, or uses network access
directly.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional, Sequence, Tuple


KEY_VALUE_RE = re.compile(r'^\s*([A-Za-z0-9_.-]+)\s*=\s*"([^"]*)"\s*(?:#.*)?$')
SECTION_RE = re.compile(r"^\s*\[([^\]]+)\]\s*(?:#.*)?$")
NUMERIC_VERSION_RE = re.compile(r"^\s*(\d+)(?:\.(\d+))?(?:\.(\d+))?")
METADATA_TIMEOUT_SECONDS = 60


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
        "--cargo-metadata",
        action="store_true",
        help=(
            "Use cargo metadata --format-version 1 --locked instead of reading "
            "package rust-version metadata from Cargo.lock."
        ),
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


def version_to_text(version: Tuple[int, int, int]) -> str:
    return f"{version[0]}.{version[1]}.{version[2]}"


def display_source(source: Optional[str]) -> str:
    if source is None:
        return "path"
    if source.startswith("registry+https://github.com/rust-lang/crates.io-index"):
        return "crates.io"
    return source


def package_record(package: LockedPackage) -> dict[str, Optional[str]]:
    return {
        "name": package.name,
        "version": package.version,
        "rust_version": package.rust_version,
        "source": display_source(package.source),
    }


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


def package_from_metadata(package: dict[str, object]) -> LockedPackage:
    source = package.get("source")
    rust_version = package.get("rust_version")
    return LockedPackage(
        name=str(package.get("name", "")),
        version=str(package.get("version", "")),
        source=source if isinstance(source, str) else None,
        rust_version=rust_version if isinstance(rust_version, str) else None,
    )


def packages_from_metadata(
    metadata: dict[str, object], warnings: List[str]
) -> List[LockedPackage]:
    packages_value = metadata.get("packages")
    if not isinstance(packages_value, list):
        warnings.append("cargo metadata output did not contain a packages array.")
        return []

    packages: List[LockedPackage] = []
    for index, package_value in enumerate(packages_value):
        if not isinstance(package_value, dict):
            warnings.append(f"Skipping metadata package at index {index}: not an object.")
            continue

        package = package_from_metadata(package_value)
        if not package.name or not package.version:
            warnings.append(
                f"Skipping metadata package at index {index}: missing name or version."
            )
            continue
        packages.append(package)
    return packages


def command_excerpt(value: str, limit: int = 4000) -> str:
    stripped = value.strip()
    if len(stripped) <= limit:
        return stripped
    return stripped[:limit] + "\n... truncated ..."


def evaluate_packages(
    *,
    source: str,
    baseline: str,
    packages: Sequence[LockedPackage],
    warnings: Sequence[str],
    include_compatible: bool,
    evidence_path: Path,
    metadata_command: Sequence[str] | None = None,
    cargo_exit_code: int | None = None,
    cargo_stderr: str | None = None,
) -> dict[str, object]:
    baseline_version = parse_numeric_version(baseline)
    if baseline_version is None:
        return {
            "name": "Andromeda MSRV Dependency Check",
            "source": source,
            "result": "error",
            "baseline_rust_version": baseline,
            "baseline_numeric": None,
            "evidence_path": str(evidence_path),
            "packages_scanned": len(packages),
            "packages_with_rust_version": 0,
            "warnings": list(warnings),
            "error": f"could not parse baseline rust-version: {baseline}",
            "metadata_command": list(metadata_command) if metadata_command else None,
            "cargo_exit_code": cargo_exit_code,
            "cargo_stderr": cargo_stderr,
        }

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

    result = "passed"
    note = "No package rust-version metadata above the baseline was detected."
    if unparseable:
        result = "error"
        note = "At least one package contains unparseable rust-version metadata."
    elif incompatible:
        result = "failed"
        note = "At least one package requires a newer rustc than the baseline."
    elif not with_metadata:
        result = "inconclusive"
        note = (
            "No package rust-version metadata was available from this evidence source."
        )

    report: dict[str, object] = {
        "name": "Andromeda MSRV Dependency Check",
        "source": source,
        "result": result,
        "baseline_rust_version": baseline,
        "baseline_numeric": version_to_text(baseline_version),
        "evidence_path": str(evidence_path),
        "packages_scanned": len(packages),
        "packages_with_rust_version": len(with_metadata),
        "incompatible_count": len(incompatible),
        "compatible_count": len(compatible),
        "unparseable_count": len(unparseable),
        "warnings": list(warnings),
        "note": note,
        "incompatible_packages": [package_record(package) for package in incompatible],
        "unparseable_packages": [package_record(package) for package in unparseable],
        "metadata_command": list(metadata_command) if metadata_command else None,
        "cargo_exit_code": cargo_exit_code,
        "cargo_stderr": cargo_stderr,
    }
    if include_compatible:
        report["compatible_packages"] = [
            package_record(package) for package in compatible
        ]
    return report


def build_lockfile_report(
    *,
    lockfile: Path,
    manifest: Path,
    baseline: str | None,
    include_compatible: bool = False,
) -> dict[str, object]:
    warnings: List[str] = []
    if not lockfile.is_file():
        return {
            "name": "Andromeda MSRV Dependency Check",
            "source": "cargo-lock",
            "result": "error",
            "baseline_rust_version": baseline,
            "baseline_numeric": None,
            "evidence_path": str(lockfile),
            "packages_scanned": 0,
            "packages_with_rust_version": 0,
            "warnings": warnings,
            "error": f"lockfile not found: {lockfile}",
        }

    effective_baseline = baseline
    if effective_baseline is None:
        if not manifest.is_file():
            return {
                "name": "Andromeda MSRV Dependency Check",
                "source": "cargo-lock",
                "result": "error",
                "baseline_rust_version": None,
                "baseline_numeric": None,
                "evidence_path": str(lockfile),
                "packages_scanned": 0,
                "packages_with_rust_version": 0,
                "warnings": warnings,
                "error": (
                    "--baseline was not provided and manifest was not found: "
                    f"{manifest}"
                ),
            }
        effective_baseline = parse_manifest_baseline(manifest)

    if effective_baseline is None:
        return {
            "name": "Andromeda MSRV Dependency Check",
            "source": "cargo-lock",
            "result": "error",
            "baseline_rust_version": None,
            "baseline_numeric": None,
            "evidence_path": str(lockfile),
            "packages_scanned": 0,
            "packages_with_rust_version": 0,
            "warnings": warnings,
            "error": "could not infer rust-version baseline; pass --baseline.",
        }

    packages, parse_warnings = parse_lockfile(lockfile)
    warnings.extend(parse_warnings)
    return evaluate_packages(
        source="cargo-lock",
        baseline=effective_baseline,
        packages=packages,
        warnings=warnings,
        include_compatible=include_compatible,
        evidence_path=lockfile,
    )


def build_cargo_metadata_report(
    *,
    manifest: Path,
    baseline: str | None = None,
    include_compatible: bool = False,
    timeout_seconds: int = METADATA_TIMEOUT_SECONDS,
) -> dict[str, object]:
    warnings: List[str] = []
    effective_baseline = baseline
    if effective_baseline is None:
        if not manifest.is_file():
            return {
                "name": "Andromeda MSRV Dependency Check",
                "source": "cargo-metadata",
                "result": "error",
                "baseline_rust_version": None,
                "baseline_numeric": None,
                "evidence_path": str(manifest),
                "packages_scanned": 0,
                "packages_with_rust_version": 0,
                "warnings": warnings,
                "error": (
                    "--baseline was not provided and manifest was not found: "
                    f"{manifest}"
                ),
                "metadata_command": None,
                "cargo_exit_code": None,
                "cargo_stderr": None,
            }
        effective_baseline = parse_manifest_baseline(manifest)

    if effective_baseline is None:
        return {
            "name": "Andromeda MSRV Dependency Check",
            "source": "cargo-metadata",
            "result": "error",
            "baseline_rust_version": None,
            "baseline_numeric": None,
            "evidence_path": str(manifest),
            "packages_scanned": 0,
            "packages_with_rust_version": 0,
            "warnings": warnings,
            "error": "could not infer rust-version baseline; pass --baseline.",
            "metadata_command": None,
            "cargo_exit_code": None,
            "cargo_stderr": None,
        }

    command = [
        "cargo",
        "metadata",
        "--format-version",
        "1",
        "--locked",
        "--manifest-path",
        str(manifest),
    ]
    if shutil.which("cargo") is None:
        return {
            "name": "Andromeda MSRV Dependency Check",
            "source": "cargo-metadata",
            "result": "error",
            "baseline_rust_version": effective_baseline,
            "baseline_numeric": None,
            "evidence_path": str(manifest),
            "packages_scanned": 0,
            "packages_with_rust_version": 0,
            "warnings": warnings,
            "error": "cargo is not available on PATH.",
            "metadata_command": command,
            "cargo_exit_code": None,
            "cargo_stderr": None,
        }

    try:
        result = subprocess.run(
            command,
            cwd=manifest.parent if manifest.parent != Path("") else Path.cwd(),
            text=True,
            encoding="utf-8",
            errors="replace",
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=timeout_seconds,
        )
    except subprocess.TimeoutExpired:
        return {
            "name": "Andromeda MSRV Dependency Check",
            "source": "cargo-metadata",
            "result": "error",
            "baseline_rust_version": effective_baseline,
            "baseline_numeric": None,
            "evidence_path": str(manifest),
            "packages_scanned": 0,
            "packages_with_rust_version": 0,
            "warnings": warnings,
            "error": f"cargo metadata timed out after {timeout_seconds} seconds.",
            "metadata_command": command,
            "cargo_exit_code": None,
            "cargo_stderr": None,
        }
    except OSError as exc:
        return {
            "name": "Andromeda MSRV Dependency Check",
            "source": "cargo-metadata",
            "result": "error",
            "baseline_rust_version": effective_baseline,
            "baseline_numeric": None,
            "evidence_path": str(manifest),
            "packages_scanned": 0,
            "packages_with_rust_version": 0,
            "warnings": warnings,
            "error": f"cargo metadata failed: {exc}",
            "metadata_command": command,
            "cargo_exit_code": None,
            "cargo_stderr": None,
        }

    stderr_excerpt = command_excerpt(result.stderr) or None
    if result.returncode != 0:
        return {
            "name": "Andromeda MSRV Dependency Check",
            "source": "cargo-metadata",
            "result": "error",
            "baseline_rust_version": effective_baseline,
            "baseline_numeric": None,
            "evidence_path": str(manifest),
            "packages_scanned": 0,
            "packages_with_rust_version": 0,
            "warnings": warnings,
            "error": "cargo metadata exited with a non-zero status.",
            "metadata_command": command,
            "cargo_exit_code": result.returncode,
            "cargo_stderr": stderr_excerpt,
        }

    if stderr_excerpt:
        warnings.append(f"cargo metadata stderr: {stderr_excerpt}")

    try:
        metadata = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        return {
            "name": "Andromeda MSRV Dependency Check",
            "source": "cargo-metadata",
            "result": "error",
            "baseline_rust_version": effective_baseline,
            "baseline_numeric": None,
            "evidence_path": str(manifest),
            "packages_scanned": 0,
            "packages_with_rust_version": 0,
            "warnings": warnings,
            "error": f"could not parse cargo metadata JSON: {exc}",
            "metadata_command": command,
            "cargo_exit_code": result.returncode,
            "cargo_stderr": stderr_excerpt,
        }

    packages = packages_from_metadata(metadata, warnings)
    return evaluate_packages(
        source="cargo-metadata",
        baseline=effective_baseline,
        packages=packages,
        warnings=warnings,
        include_compatible=include_compatible,
        evidence_path=manifest,
        metadata_command=command,
        cargo_exit_code=result.returncode,
        cargo_stderr=stderr_excerpt,
    )


def print_package_table(title: str, packages: Sequence[dict[str, Optional[str]]]) -> None:
    if not packages:
        return

    print(title)
    print("package\tversion\trust-version\tsource")
    for package in packages:
        print(
            f"{package['name']}\t{package['version']}\t"
            f"{package['rust_version']}\t{package['source']}"
        )


def print_report(report: dict[str, object], show_compatible: bool) -> None:
    print("MSRV dependency check")
    print(f"source={report['source']}")
    print(f"baseline_rust_version={report['baseline_rust_version']}")
    if report["source"] == "cargo-lock":
        print(f"lockfile={report['evidence_path']}")
    else:
        print(f"manifest={report['evidence_path']}")
        command = report.get("metadata_command")
        if isinstance(command, list):
            print(f"metadata_command={' '.join(str(part) for part in command)}")
        if report.get("cargo_exit_code") is not None:
            print(f"cargo_exit_code={report['cargo_exit_code']}")
    print(f"packages_scanned={report['packages_scanned']}")
    print(f"packages_with_rust_version={report['packages_with_rust_version']}")

    warnings = report.get("warnings", [])
    if warnings:
        print("warnings:")
        for warning in warnings:
            print(f"- {warning}")

    if report["result"] == "error":
        print("result=error")
        if report.get("error"):
            print(f"error={report['error']}")
        print_package_table(
            "Packages with unparseable rust-version metadata:",
            report.get("unparseable_packages", []),
        )
        return

    if report["result"] == "failed":
        print("result=failed")
        print_package_table(
            "Packages requiring a newer rustc than the baseline:",
            report.get("incompatible_packages", []),
        )
        if show_compatible:
            print_package_table(
                "Compatible packages with rust-version metadata:",
                report.get("compatible_packages", []),
            )
        return

    if report["result"] == "inconclusive":
        print("result=inconclusive")
        print(report["note"])
        return

    print("result=passed")
    print(report["note"])
    if show_compatible:
        print_package_table(
            "Compatible packages with rust-version metadata:",
            report.get("compatible_packages", []),
        )


def report_exit_code(report: dict[str, object]) -> int:
    if report["result"] == "failed":
        return 1
    if report["result"] == "error":
        return 2
    return 0


def main(argv: Sequence[str]) -> int:
    args = parse_args(argv)
    lockfile = Path(args.lockfile)
    manifest = Path(args.manifest)

    if args.cargo_metadata:
        report = build_cargo_metadata_report(
            manifest=manifest,
            baseline=args.baseline,
            include_compatible=args.show_compatible,
        )
    else:
        report = build_lockfile_report(
            lockfile=lockfile,
            manifest=manifest,
            baseline=args.baseline,
            include_compatible=args.show_compatible,
        )

    print_report(report, args.show_compatible)
    return report_exit_code(report)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
