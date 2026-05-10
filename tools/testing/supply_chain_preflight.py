#!/usr/bin/env python3
"""Read-only local supply-chain tooling preflight for Andromeda."""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Sequence

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - exercised only on old Python.
    tomllib = None

import msrv_dependency_check


ROOT = Path(__file__).resolve().parents[2]
PROBE_TIMEOUT_SECONDS = 10
REPORT_COMMAND_TIMEOUT_SECONDS = 60
RAW_OUTPUT_EXCERPT_LINES = 120

TOOL_REQUIRED_LOCAL = "required-local"
TOOL_REQUIRED_CI = "required-ci"
TOOL_PLANNED = "planned"

C5_DURABLE_CRATES = (
    "andromeda-wal",
    "andromeda-storage",
    "andromeda-transaction",
    "andromeda-transaction-log",
    "andromeda-mvcc",
    "andromeda-locking",
    "andromeda-savepoint",
)

DEPENDENCY_SECTIONS = (
    "dependencies",
    "dev-dependencies",
    "build-dependencies",
)

WATCH_WORKFLOWS = (
    ".github/workflows/05-supply-chain.yml",
    ".github/workflows/release-gate-chain.yml",
)

WATCH_EDGE_PATTERNS = (
    ("Cargo.toml", "root workspace dependency, MSRV, and feature policy"),
    ("Cargo.lock", "resolved dependency graph and transitive package versions"),
    ("crates/**/Cargo.toml", "member dependency and feature declarations"),
    ("deny.toml", "license, source, duplicate, advisory, and ban policy"),
    (
        "docs/adr/ADR-0010-SUPPLY_CHAIN_POLICY.md",
        "human governance policy for supply-chain release gates",
    ),
    (
        "tools/testing/supply_chain_preflight.py",
        "local preflight reporting logic",
    ),
    (
        "tools/testing/msrv_dependency_check.py",
        "dependency MSRV metadata reporting logic",
    ),
    ("rust-toolchain.toml", "pinned release-validation Rust toolchain"),
    (
        ".github/workflows/05-supply-chain.yml",
        "dedicated supply-chain workflow definition",
    ),
    (
        ".github/workflows/release-gate-chain.yml",
        "release gate chain workflow definition",
    ),
)

DUPLICATE_ROOT_RE = re.compile(r"^(?P<name>[A-Za-z0-9_.+-]+) v(?P<version>[^\s]+)")
WORKSPACE_OWNER_RE = re.compile(
    r"(?P<crate>andromeda-[A-Za-z0-9_-]+) v(?P<version>[^\s]+) \((?P<path>[^)]+)\)"
)


@dataclass(frozen=True)
class ToolSpec:
    key: str
    label: str
    probe_command: tuple[str, ...]
    path_executable: str | None
    expected_gate: str
    absence_blocks: str
    classification: str


@dataclass(frozen=True)
class ToolResult:
    key: str
    label: str
    available: bool
    path: str | None
    probe_command: tuple[str, ...]
    probe_exit_code: int | None
    version_or_help: str | None
    absence_blocks: str
    expected_gate: str
    note: str
    classification: str


TOOL_SPECS = (
    ToolSpec(
        key="cargo-nextest",
        label="cargo-nextest",
        probe_command=("cargo", "nextest", "--version"),
        path_executable="cargo-nextest",
        expected_gate="cargo nextest run --workspace --all-features",
        absence_blocks=(
            "Blocks the preferred local workspace test gate and any release "
            "evidence that depends on nextest output, profiles, retries, or archives."
        ),
        classification=TOOL_REQUIRED_LOCAL,
    ),
    ToolSpec(
        key="cargo-audit",
        label="cargo-audit",
        probe_command=("cargo", "audit", "--version"),
        path_executable="cargo-audit",
        expected_gate="cargo audit --deny warnings",
        absence_blocks=(
            "Blocks local RustSec advisory evidence and release review of "
            "unreviewed vulnerability or warning findings."
        ),
        classification=TOOL_REQUIRED_CI,
    ),
    ToolSpec(
        key="cargo-deny",
        label="cargo-deny",
        probe_command=("cargo", "deny", "--version"),
        path_executable="cargo-deny",
        expected_gate="cargo deny check --all-features",
        absence_blocks=(
            "Blocks local dependency policy evidence for licenses, sources, "
            "banned crates, duplicate versions, and advisory policy checks."
        ),
        classification=TOOL_REQUIRED_CI,
    ),
    ToolSpec(
        key="cargo-vet",
        label="cargo-vet",
        probe_command=("cargo", "vet", "--version"),
        path_executable="cargo-vet",
        expected_gate="cargo vet",
        absence_blocks=(
            "Blocks local cargo-vet attestation and exemption evidence when "
            "vet governance is configured or required for a release packet."
        ),
        classification=TOOL_PLANNED,
    ),
    ToolSpec(
        key="cargo-tree",
        label="cargo tree",
        probe_command=("cargo", "tree", "--help"),
        path_executable=None,
        expected_gate="cargo tree --workspace --locked",
        absence_blocks=(
            "Blocks local transitive dependency graph triage, including owner "
            "paths for advisories, duplicate versions, license concerns, and MSRV drift."
        ),
        classification=TOOL_REQUIRED_LOCAL,
    ),
)


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Read-only Andromeda supply-chain tooling preflight. The script only "
            "checks local command availability and never installs tools."
        ),
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit machine-readable JSON instead of text.",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit with status 1 when any expected supply-chain tool is missing.",
    )
    return parser.parse_args(argv)


def first_nonempty_line(value: str) -> str | None:
    for line in value.splitlines():
        stripped = line.strip()
        if stripped:
            return stripped
    return None


def display_command(command: Sequence[str]) -> str:
    return " ".join(command)


def relative_path(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return str(path)


def raw_excerpt(value: str, limit: int = RAW_OUTPUT_EXCERPT_LINES) -> list[str]:
    lines = [line.rstrip() for line in value.splitlines() if line.strip()]
    if len(lines) <= limit:
        return lines
    return lines[:limit] + [f"... truncated {len(lines) - limit} more lines ..."]


def run_probe(command: tuple[str, ...]) -> tuple[int | None, str | None, str]:
    if shutil.which(command[0]) is None:
        return None, None, f"{command[0]} is not available on PATH"

    try:
        result = subprocess.run(
            list(command),
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            errors="replace",
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=PROBE_TIMEOUT_SECONDS,
        )
    except subprocess.TimeoutExpired:
        return None, None, f"probe timed out after {PROBE_TIMEOUT_SECONDS} seconds"
    except OSError as exc:
        return None, None, f"probe failed: {exc}"

    summary = first_nonempty_line(result.stdout) or first_nonempty_line(result.stderr)
    if result.returncode == 0:
        return result.returncode, summary, "available"

    note = summary or f"probe exited with status {result.returncode}"
    return result.returncode, summary, note


def run_report_command(command: Sequence[str]) -> dict[str, Any]:
    if shutil.which(command[0]) is None:
        return {
            "available": False,
            "exit_code": None,
            "stdout": "",
            "stderr": f"{command[0]} is not available on PATH",
            "note": f"{command[0]} is not available on PATH",
        }

    try:
        result = subprocess.run(
            list(command),
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            errors="replace",
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=REPORT_COMMAND_TIMEOUT_SECONDS,
        )
    except subprocess.TimeoutExpired:
        note = f"command timed out after {REPORT_COMMAND_TIMEOUT_SECONDS} seconds"
        return {
            "available": True,
            "exit_code": None,
            "stdout": "",
            "stderr": "",
            "note": note,
        }
    except OSError as exc:
        return {
            "available": True,
            "exit_code": None,
            "stdout": "",
            "stderr": str(exc),
            "note": f"command failed: {exc}",
        }

    note = "available" if result.returncode == 0 else "command returned non-zero"
    return {
        "available": True,
        "exit_code": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "note": note,
    }


def check_tool(spec: ToolSpec) -> ToolResult:
    path = shutil.which(spec.path_executable) if spec.path_executable else None
    probe_exit_code, summary, note = run_probe(spec.probe_command)
    available = probe_exit_code == 0
    if available and path is None:
        path = shutil.which(spec.probe_command[0])

    return ToolResult(
        key=spec.key,
        label=spec.label,
        available=available,
        path=path,
        probe_command=spec.probe_command,
        probe_exit_code=probe_exit_code,
        version_or_help=summary,
        absence_blocks=spec.absence_blocks,
        expected_gate=spec.expected_gate,
        note=note,
        classification=spec.classification,
    )


def check_tools() -> list[ToolResult]:
    return [check_tool(spec) for spec in TOOL_SPECS]


def compact_tool_record(result: ToolResult) -> dict[str, object]:
    return {
        "key": result.key,
        "label": result.label,
        "classification": result.classification,
        "expected_gate": result.expected_gate,
        "absence_blocks": result.absence_blocks,
        "note": result.note,
    }


def build_missing_tool_classification(
    results: Sequence[ToolResult],
) -> dict[str, object]:
    categories = {
        TOOL_REQUIRED_LOCAL: [],
        TOOL_REQUIRED_CI: [],
        TOOL_PLANNED: [],
    }
    for result in results:
        if not result.available:
            categories.setdefault(result.classification, []).append(
                compact_tool_record(result)
            )

    return {
        "mode": "report-only",
        "required_local": categories[TOOL_REQUIRED_LOCAL],
        "required_ci": categories[TOOL_REQUIRED_CI],
        "planned": categories[TOOL_PLANNED],
        "counts": {
            "required_local": len(categories[TOOL_REQUIRED_LOCAL]),
            "required_ci": len(categories[TOOL_REQUIRED_CI]),
            "planned": len(categories[TOOL_PLANNED]),
        },
        "note": (
            "Classifications explain preflight impact. The default report-only "
            "mode records missing tools without installing or changing policy."
        ),
    }


def parse_duplicate_tree(stdout: str) -> list[dict[str, object]]:
    groups: dict[str, dict[str, Any]] = {}
    current: dict[str, Any] | None = None

    for raw_line in stdout.splitlines():
        if not raw_line.strip():
            continue

        root_match = DUPLICATE_ROOT_RE.match(raw_line)
        if root_match is not None:
            name = root_match.group("name")
            version = root_match.group("version")
            group = groups.setdefault(
                name,
                {
                    "dependency": name,
                    "versions": {},
                    "root_occurrences": 0,
                },
            )
            group["root_occurrences"] += 1
            version_entry = group["versions"].setdefault(
                version,
                {
                    "version": version,
                    "owner_crates": set(),
                    "owner_paths": set(),
                    "sample_paths": [],
                },
            )
            current = version_entry
            continue

        if current is None:
            continue

        owner_match = WORKSPACE_OWNER_RE.search(raw_line)
        if owner_match is None:
            continue

        owner_crate = owner_match.group("crate")
        owner_path = owner_match.group("path")
        current["owner_crates"].add(owner_crate)
        current["owner_paths"].add(owner_path)
        if len(current["sample_paths"]) < 6:
            current["sample_paths"].append(raw_line.strip())

    duplicate_groups: list[dict[str, object]] = []
    for group in groups.values():
        versions = []
        all_owner_crates: set[str] = set()
        all_owner_paths: set[str] = set()
        for version_entry in group["versions"].values():
            owner_crates = sorted(version_entry["owner_crates"])
            owner_paths = sorted(version_entry["owner_paths"])
            all_owner_crates.update(owner_crates)
            all_owner_paths.update(owner_paths)
            versions.append(
                {
                    "version": version_entry["version"],
                    "owner_crates": owner_crates,
                    "owner_paths": owner_paths,
                    "sample_paths": version_entry["sample_paths"],
                }
            )

        versions.sort(key=lambda item: str(item["version"]))
        duplicate_groups.append(
            {
                "dependency": group["dependency"],
                "version_count": len(versions),
                "root_occurrences": group["root_occurrences"],
                "owner_crates": sorted(all_owner_crates),
                "owner_paths": sorted(all_owner_paths),
                "versions": versions,
            }
        )

    duplicate_groups.sort(
        key=lambda item: (str(item["dependency"]), int(item["version_count"]))
    )
    return duplicate_groups


def build_duplicate_dependency_report() -> dict[str, object]:
    command = ("cargo", "tree", "-d", "--workspace", "--locked")
    result = run_report_command(command)
    report: dict[str, object] = {
        "mode": "report-only",
        "command": list(command),
        "available": result["available"],
        "exit_code": result["exit_code"],
        "status": "unavailable",
        "note": result["note"],
        "duplicate_group_count": 0,
        "duplicate_groups": [],
        "stdout_excerpt": [],
        "stderr_excerpt": raw_excerpt(str(result["stderr"])),
    }

    if not result["available"]:
        return report
    if result["exit_code"] != 0:
        report["status"] = "command-failed"
        return report

    duplicate_groups = parse_duplicate_tree(str(result["stdout"]))
    report.update(
        {
            "status": "duplicates-found" if duplicate_groups else "no-duplicates",
            "duplicate_group_count": len(duplicate_groups),
            "duplicate_groups": duplicate_groups,
            "stdout_excerpt": raw_excerpt(str(result["stdout"])),
        }
    )
    return report


def load_toml(path: Path) -> tuple[dict[str, Any] | None, str | None]:
    if tomllib is None:
        return None, "tomllib is not available in this Python runtime."
    try:
        with path.open("rb") as handle:
            return tomllib.load(handle), None
    except OSError as exc:
        return None, f"could not read {relative_path(path)}: {exc}"
    except tomllib.TOMLDecodeError as exc:
        return None, f"could not parse {relative_path(path)}: {exc}"


def workspace_data() -> tuple[dict[str, Any] | None, str | None]:
    return load_toml(ROOT / "Cargo.toml")


def workspace_members(root_data: dict[str, Any]) -> list[Path]:
    workspace = root_data.get("workspace", {})
    if not isinstance(workspace, dict):
        return []

    members = workspace.get("members", [])
    if not isinstance(members, list):
        return []

    manifests: list[Path] = []
    for member in members:
        if isinstance(member, str):
            manifests.append(ROOT / member / "Cargo.toml")
    return manifests


def workspace_dependency_table(root_data: dict[str, Any]) -> dict[str, Any]:
    workspace = root_data.get("workspace", {})
    if not isinstance(workspace, dict):
        return {}
    dependencies = workspace.get("dependencies", {})
    return dependencies if isinstance(dependencies, dict) else {}


def manifest_crate_name(manifest: Path, data: dict[str, Any]) -> str:
    package = data.get("package", {})
    if isinstance(package, dict):
        name = package.get("name")
        if isinstance(name, str):
            return name
    return manifest.parent.name


def iter_dependency_sections(
    data: dict[str, Any],
) -> list[tuple[str, str | None, dict[str, Any]]]:
    sections: list[tuple[str, str | None, dict[str, Any]]] = []
    for section_name in DEPENDENCY_SECTIONS:
        section = data.get(section_name)
        if isinstance(section, dict):
            sections.append((section_name, None, section))

    target = data.get("target", {})
    if isinstance(target, dict):
        for target_name, target_data in target.items():
            if not isinstance(target_name, str) or not isinstance(target_data, dict):
                continue
            for section_name in DEPENDENCY_SECTIONS:
                section = target_data.get(section_name)
                if isinstance(section, dict):
                    sections.append((section_name, target_name, section))
    return sections


def dependency_uses_workspace(spec: Any) -> bool:
    return isinstance(spec, dict) and spec.get("workspace") is True


def dependency_path(spec: Any) -> str | None:
    if isinstance(spec, dict):
        path = spec.get("path")
        if isinstance(path, str):
            return path
    return None


def default_features_state(spec: Any) -> str:
    if not isinstance(spec, dict):
        return "implicit"
    value = spec.get("default-features")
    if value is False:
        return "disabled"
    if value is True:
        return "enabled"
    return "implicit"


def effective_default_features_state(spec: Any, workspace_spec: Any | None) -> str:
    member_state = default_features_state(spec)
    workspace_state = default_features_state(workspace_spec)
    if member_state == "disabled":
        return "disabled"
    if dependency_uses_workspace(spec) and workspace_state == "disabled":
        return "disabled"
    if member_state == "enabled" or workspace_state == "enabled":
        return "enabled"
    return "implicit"


def is_internal_dependency(
    name: str,
    spec: Any,
    workspace_dependencies: dict[str, Any],
) -> bool:
    if name.startswith("andromeda-"):
        return True

    paths = [
        dependency_path(spec),
        dependency_path(workspace_dependencies.get(name)),
    ]
    return any(path is not None and path.startswith("crates/") for path in paths)


def spec_summary(spec: Any) -> dict[str, object]:
    if isinstance(spec, str):
        return {"kind": "version", "version": spec}
    if not isinstance(spec, dict):
        return {"kind": type(spec).__name__}

    keys = (
        "workspace",
        "path",
        "version",
        "default-features",
        "features",
        "optional",
        "package",
    )
    summary: dict[str, object] = {"kind": "table"}
    for key in keys:
        if key in spec:
            summary[key] = spec[key]
    return summary


def dependency_declaration_record(
    *,
    crate: str,
    manifest: Path,
    section: str,
    target: str | None,
    dependency: str,
    spec: Any,
    workspace_spec: Any | None,
    workspace_dependencies: dict[str, Any],
) -> dict[str, object]:
    return {
        "crate": crate,
        "manifest": relative_path(manifest),
        "section": section,
        "target": target,
        "dependency": dependency,
        "uses_workspace_dependency": dependency_uses_workspace(spec),
        "workspace_dependency_declared": dependency in workspace_dependencies,
        "internal_dependency": is_internal_dependency(
            dependency, spec, workspace_dependencies
        ),
        "member_spec": spec_summary(spec),
        "workspace_spec": spec_summary(workspace_spec)
        if workspace_spec is not None
        else None,
    }


def collect_workspace_dependency_declarations(
    root_data: dict[str, Any],
) -> tuple[list[dict[str, object]], list[str]]:
    workspace_dependencies = workspace_dependency_table(root_data)
    declarations: list[dict[str, object]] = []
    warnings: list[str] = []

    for manifest in workspace_members(root_data):
        manifest_data, error = load_toml(manifest)
        if error is not None:
            warnings.append(error)
            continue
        if manifest_data is None:
            continue

        crate = manifest_crate_name(manifest, manifest_data)
        for section, target, dependencies in iter_dependency_sections(manifest_data):
            for dependency, spec in dependencies.items():
                if not isinstance(dependency, str):
                    continue
                workspace_spec = workspace_dependencies.get(dependency)
                declarations.append(
                    dependency_declaration_record(
                        crate=crate,
                        manifest=manifest,
                        section=section,
                        target=target,
                        dependency=dependency,
                        spec=spec,
                        workspace_spec=workspace_spec,
                        workspace_dependencies=workspace_dependencies,
                    )
                )

    return declarations, warnings


def build_c5_default_features_report(root_data: dict[str, Any]) -> dict[str, object]:
    workspace_dependencies = workspace_dependency_table(root_data)
    findings: list[dict[str, object]] = []
    scanned_external = 0
    skipped_internal = 0
    warnings: list[str] = []

    for crate in C5_DURABLE_CRATES:
        manifest = ROOT / "crates" / crate / "Cargo.toml"
        manifest_data, error = load_toml(manifest)
        if error is not None:
            warnings.append(error)
            continue
        if manifest_data is None:
            continue

        for section, target, dependencies in iter_dependency_sections(manifest_data):
            for dependency, spec in dependencies.items():
                if not isinstance(dependency, str):
                    continue
                if is_internal_dependency(dependency, spec, workspace_dependencies):
                    skipped_internal += 1
                    continue

                scanned_external += 1
                workspace_spec = workspace_dependencies.get(dependency)
                effective_state = effective_default_features_state(spec, workspace_spec)
                if effective_state == "disabled":
                    continue

                findings.append(
                    {
                        "crate": crate,
                        "manifest": relative_path(manifest),
                        "section": section,
                        "target": target,
                        "dependency": dependency,
                        "effective_default_features": effective_state,
                        "member_default_features": default_features_state(spec),
                        "workspace_default_features": default_features_state(
                            workspace_spec
                        )
                        if workspace_spec is not None
                        else None,
                        "uses_workspace_dependency": dependency_uses_workspace(spec),
                        "workspace_dependency_declared": dependency
                        in workspace_dependencies,
                        "note": (
                            "Report-only finding: C5 external dependency defaults "
                            "are not explicitly disabled at the effective declaration."
                        ),
                    }
                )

    return {
        "mode": "report-only",
        "c5_crates": list(C5_DURABLE_CRATES),
        "status": "findings" if findings else "clean",
        "external_dependency_count": scanned_external,
        "skipped_internal_dependency_count": skipped_internal,
        "finding_count": len(findings),
        "findings": findings,
        "warnings": warnings,
    }


def build_workspace_centralization_report(
    root_data: dict[str, Any],
) -> dict[str, object]:
    workspace_dependencies = workspace_dependency_table(root_data)
    declarations, warnings = collect_workspace_dependency_declarations(root_data)
    workspace_bypasses: list[dict[str, object]] = []
    direct_external: dict[str, list[dict[str, object]]] = {}
    local_path_not_workspace: list[dict[str, object]] = []

    for declaration in declarations:
        dependency = str(declaration["dependency"])
        workspace_declared = bool(declaration["workspace_dependency_declared"])
        uses_workspace = bool(declaration["uses_workspace_dependency"])
        internal = bool(declaration["internal_dependency"])
        member_spec = declaration["member_spec"]

        if workspace_declared and not uses_workspace:
            workspace_bypasses.append(declaration)

        if not workspace_declared and not uses_workspace and not internal:
            direct_external.setdefault(dependency, []).append(declaration)

        if internal and not workspace_declared and not uses_workspace:
            path = None
            if isinstance(member_spec, dict):
                value = member_spec.get("path")
                path = value if isinstance(value, str) else None
            if path is not None:
                local_path_not_workspace.append(declaration)

    repeated_external_candidates = []
    for dependency, owners in direct_external.items():
        owner_crates = sorted({str(owner["crate"]) for owner in owners})
        if len(owner_crates) < 2:
            continue
        repeated_external_candidates.append(
            {
                "dependency": dependency,
                "owner_count": len(owner_crates),
                "owner_crates": owner_crates,
                "declarations": owners,
            }
        )

    repeated_external_candidates.sort(key=lambda item: str(item["dependency"]))
    workspace_bypasses.sort(
        key=lambda item: (str(item["dependency"]), str(item["crate"]))
    )
    local_path_not_workspace.sort(
        key=lambda item: (str(item["dependency"]), str(item["crate"]))
    )

    return {
        "mode": "report-only",
        "status": "findings"
        if workspace_bypasses
        or repeated_external_candidates
        or local_path_not_workspace
        else "clean",
        "workspace_dependency_count": len(workspace_dependencies),
        "declaration_count": len(declarations),
        "workspace_dependency_bypass_count": len(workspace_bypasses),
        "repeated_external_candidate_count": len(repeated_external_candidates),
        "local_path_not_workspace_count": len(local_path_not_workspace),
        "workspace_dependency_bypasses": workspace_bypasses,
        "repeated_external_candidates": repeated_external_candidates,
        "local_path_not_workspace": local_path_not_workspace,
        "warnings": warnings,
        "note": (
            "Findings are centralization hints only. This preflight does not edit "
            "Cargo manifests or workspace dependency declarations."
        ),
    }


def build_toml_unavailable_report(name: str, error: str | None) -> dict[str, object]:
    return {
        "mode": "report-only",
        "status": "unavailable",
        "name": name,
        "error": error or "workspace Cargo.toml could not be loaded.",
        "warnings": [],
    }


def build_watch_edge_report() -> dict[str, object]:
    workflow_records: list[dict[str, object]] = []
    missing_edges: list[dict[str, object]] = []

    for workflow in WATCH_WORKFLOWS:
        workflow_path = ROOT / workflow
        try:
            content = workflow_path.read_text(encoding="utf-8")
            read_error = None
        except OSError as exc:
            content = ""
            read_error = f"could not read {workflow}: {exc}"

        edge_records = []
        for pattern, reason in WATCH_EDGE_PATTERNS:
            watched = pattern in content
            record = {
                "pattern": pattern,
                "reason": reason,
                "watched": watched,
            }
            edge_records.append(record)
            if not watched:
                missing_edges.append(
                    {
                        "workflow": workflow,
                        "pattern": pattern,
                        "reason": reason,
                    }
                )

        workflow_records.append(
            {
                "workflow": workflow,
                "read_error": read_error,
                "edges": edge_records,
            }
        )

    return {
        "mode": "report-only",
        "status": "watch-gaps-found" if missing_edges else "covered",
        "workflow_count": len(workflow_records),
        "edge_count": len(WATCH_EDGE_PATTERNS),
        "missing_edge_count": len(missing_edges),
        "missing_edges": missing_edges,
        "workflows": workflow_records,
        "note": (
            "Watch-edge findings compare expected supply-chain-sensitive paths "
            "with workflow path filters. They do not edit workflow triggers."
        ),
    }


def build_msrv_metadata_report() -> dict[str, object]:
    report = msrv_dependency_check.build_cargo_metadata_report(
        manifest=ROOT / "Cargo.toml",
        include_compatible=False,
    )
    report["mode"] = "report-only"
    report["note"] = (
        str(report.get("note", ""))
        + " This section is advisory preflight evidence from cargo metadata."
    ).strip()
    return report


def build_report(results: Sequence[ToolResult], strict: bool) -> dict[str, object]:
    missing = [result for result in results if not result.available]
    root_data, root_error = workspace_data()

    if root_data is None:
        c5_default_features = build_toml_unavailable_report(
            "C5 default-features report", root_error
        )
        workspace_centralization = build_toml_unavailable_report(
            "Workspace dependency centralization report", root_error
        )
    else:
        c5_default_features = build_c5_default_features_report(root_data)
        workspace_centralization = build_workspace_centralization_report(root_data)

    return {
        "name": "Andromeda Supply-Chain Tooling Preflight",
        "repository": str(ROOT),
        "mode": "strict" if strict else "report-only",
        "status": "blocked" if missing else "ready",
        "missing_count": len(missing),
        "available_count": len(results) - len(missing),
        "checked_count": len(results),
        "strict_failure": bool(strict and missing),
        "tools": [asdict(result) for result in results],
        "missing_tool_classification": build_missing_tool_classification(results),
        "duplicate_dependency_owners": build_duplicate_dependency_report(),
        "c5_default_features": c5_default_features,
        "workspace_dependency_centralization": workspace_centralization,
        "watch_edges": build_watch_edge_report(),
        "msrv_metadata": build_msrv_metadata_report(),
    }


def print_text_list(prefix: str, values: Sequence[str], limit: int = 8) -> None:
    for value in values[:limit]:
        print(f"{prefix}{value}")
    if len(values) > limit:
        print(f"{prefix}... {len(values) - limit} more")


def print_text(report: dict[str, object]) -> None:
    print(report["name"])
    print(f"Repository: {report['repository']}")
    print(f"Mode: {report['mode']}")
    print(f"Status: {report['status']}")
    print()

    print("Tool Availability")
    for result_data in report["tools"]:
        result = ToolResult(**result_data)
        status = "found" if result.available else "missing"
        path = f" ({result.path})" if result.path else ""
        print(f"  {result.label}: {status}{path}")
        print(f"    classification: {result.classification}")
        print(f"    probe: {display_command(result.probe_command)}")
        if result.version_or_help:
            print(f"    evidence: {result.version_or_help}")
        if not result.available:
            print(f"    blocks: {result.absence_blocks}")
            print(f"    expected gate: {result.expected_gate}")
            print(f"    note: {result.note}")
    print()

    classification = report["missing_tool_classification"]
    print("Missing Tool Classification")
    for key in ("required_local", "required_ci", "planned"):
        entries = classification[key]
        labels = [entry["label"] for entry in entries]
        display = ", ".join(labels) if labels else "none"
        print(f"  {key}: {display}")
    print()

    if report["missing_count"]:
        print("Missing Tool Impact")
        for result_data in report["tools"]:
            result = ToolResult(**result_data)
            if not result.available:
                print(f"  {result.label}: {result.absence_blocks}")
        print()

    duplicate_report = report["duplicate_dependency_owners"]
    print("Duplicate Dependency Owners")
    print(f"  status: {duplicate_report['status']}")
    print(f"  command: {display_command(duplicate_report['command'])}")
    print(f"  groups: {duplicate_report['duplicate_group_count']}")
    for group in duplicate_report["duplicate_groups"][:8]:
        versions = ", ".join(
            str(version["version"]) for version in group["versions"]
        )
        owners = ", ".join(group["owner_crates"]) or "owner not detected"
        print(f"  {group['dependency']}: {versions}; owners: {owners}")
    if duplicate_report["duplicate_group_count"] > 8:
        print(
            "  ... "
            f"{duplicate_report['duplicate_group_count'] - 8} more duplicate groups"
        )
    print()

    c5_report = report["c5_default_features"]
    print("C5 Default-Features Report")
    print(f"  status: {c5_report['status']}")
    print(f"  findings: {c5_report.get('finding_count', 0)}")
    for finding in c5_report.get("findings", [])[:8]:
        print(
            "  "
            f"{finding['crate']} {finding['section']} {finding['dependency']}: "
            f"{finding['effective_default_features']}"
        )
    if c5_report.get("finding_count", 0) > 8:
        print(f"  ... {c5_report['finding_count'] - 8} more findings")
    print()

    centralization = report["workspace_dependency_centralization"]
    print("Workspace Dependency Centralization")
    print(f"  status: {centralization['status']}")
    print(
        "  findings: "
        f"{centralization.get('workspace_dependency_bypass_count', 0)} bypasses, "
        f"{centralization.get('repeated_external_candidate_count', 0)} repeated external candidates, "
        f"{centralization.get('local_path_not_workspace_count', 0)} local paths"
    )
    for finding in centralization.get("workspace_dependency_bypasses", [])[:6]:
        print(
            "  bypass: "
            f"{finding['crate']} declares {finding['dependency']} outside workspace = true"
        )
    for finding in centralization.get("repeated_external_candidates", [])[:6]:
        owners = ", ".join(finding["owner_crates"])
        print(f"  candidate: {finding['dependency']} in {owners}")
    print()

    watch_edges = report["watch_edges"]
    print("Watch Edges")
    print(f"  status: {watch_edges['status']}")
    print(f"  missing edges: {watch_edges['missing_edge_count']}")
    for edge in watch_edges["missing_edges"][:8]:
        print(f"  {edge['workflow']}: missing {edge['pattern']}")
    if watch_edges["missing_edge_count"] > 8:
        print(f"  ... {watch_edges['missing_edge_count'] - 8} more watch gaps")
    print()

    msrv_report = report["msrv_metadata"]
    print("MSRV Metadata")
    print(f"  result: {msrv_report['result']}")
    print(f"  baseline: {msrv_report['baseline_rust_version']}")
    print(f"  packages scanned: {msrv_report['packages_scanned']}")
    print(f"  packages with rust_version: {msrv_report['packages_with_rust_version']}")
    if msrv_report.get("incompatible_count", 0):
        for package in msrv_report.get("incompatible_packages", [])[:8]:
            print(
                "  incompatible: "
                f"{package['name']} {package['version']} requires {package['rust_version']}"
            )
    if msrv_report.get("error"):
        print(f"  error: {msrv_report['error']}")
    print()

    print(
        "Result: "
        f"{report['available_count']} available, "
        f"{report['missing_count']} missing, "
        f"{report['checked_count']} checked."
    )


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    results = check_tools()
    report = build_report(results, args.strict)

    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print_text(report)

    return 1 if report["strict_failure"] else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
