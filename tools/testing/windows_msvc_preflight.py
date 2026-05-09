#!/usr/bin/env python3
"""Read-only Windows/MSVC linker preflight for Andromeda Rust gates."""

from __future__ import annotations

import argparse
import json
import os
import platform
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any, Sequence


ROOT = Path(__file__).resolve().parents[2]
MAX_CANDIDATES = 20


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Expose Windows/MSVC Rust linker readiness without changing the machine.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit a machine-readable report.",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit with status 1 when a blocking Windows/MSVC requirement is missing.",
    )
    parser.add_argument(
        "--target",
        help=(
            "Rust target triple to inspect. Defaults to CARGO_BUILD_TARGET, "
            "then .cargo/config*, then rustc host."
        ),
    )
    return parser.parse_args(argv)


def env_value(name: str) -> str | None:
    value = os.environ.get(name)
    if value is None or value == "":
        return None
    return value


def path_to_str(path: Path | None) -> str | None:
    if path is None:
        return None
    return str(path)


def safe_exists(path: Path) -> bool:
    try:
        return path.exists()
    except OSError:
        return False


def safe_is_file(path: Path) -> bool:
    try:
        return path.is_file()
    except OSError:
        return False


def safe_is_dir(path: Path) -> bool:
    try:
        return path.is_dir()
    except OSError:
        return False


def safe_iterdir(path: Path) -> list[Path]:
    try:
        return sorted(path.iterdir(), key=lambda item: item.name.lower())
    except OSError:
        return []


def run_command(command: Sequence[str], timeout_seconds: int = 10) -> dict[str, Any]:
    try:
        completed = subprocess.run(
            list(command),
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=timeout_seconds,
        )
    except OSError as exc:
        return {
            "command": list(command),
            "returncode": None,
            "stdout": "",
            "stderr": "",
            "error": str(exc),
        }
    except subprocess.TimeoutExpired as exc:
        return {
            "command": list(command),
            "returncode": None,
            "stdout": exc.stdout or "",
            "stderr": exc.stderr or "",
            "error": f"timed out after {timeout_seconds} seconds",
        }

    return {
        "command": list(command),
        "returncode": completed.returncode,
        "stdout": completed.stdout.strip(),
        "stderr": completed.stderr.strip(),
        "error": None,
    }


def parse_rustc_verbose(output: str) -> dict[str, str | None]:
    details: dict[str, str | None] = {
        "version": None,
        "host": None,
        "release": None,
        "commit_hash": None,
        "llvm_version": None,
    }
    for line in output.splitlines():
        if line.startswith("rustc "):
            details["version"] = line.strip()
            continue
        key, _, value = line.partition(":")
        key = key.strip()
        value = value.strip()
        if key == "host":
            details["host"] = value
        elif key == "release":
            details["release"] = value
        elif key == "commit-hash":
            details["commit_hash"] = value
        elif key == "LLVM version":
            details["llvm_version"] = value
    return details


def parse_cargo_config_target(path: Path) -> str | None:
    if not safe_is_file(path):
        return None

    section = ""
    target_re = re.compile(r'^\s*target\s*=\s*"([^"]+)"\s*(?:#.*)?$')
    section_re = re.compile(r"^\s*\[([^\]]+)\]\s*(?:#.*)?$")

    try:
        lines = path.read_text(encoding="utf-8", errors="ignore").splitlines()
    except OSError:
        return None

    for line in lines:
        section_match = section_re.match(line)
        if section_match is not None:
            section = section_match.group(1).strip()
            continue

        if section != "build":
            continue

        target_match = target_re.match(line)
        if target_match is not None:
            return target_match.group(1)

    return None


def cargo_config_target(root: Path) -> tuple[str | None, str | None]:
    for relative in (".cargo/config.toml", ".cargo/config"):
        path = root / relative
        target = parse_cargo_config_target(path)
        if target is not None:
            return target, relative
    return None, None


def selected_target(
    cli_target: str | None,
    rustc_host: str | None,
    root: Path,
) -> tuple[str | None, str]:
    if cli_target:
        return cli_target, "cli"

    cargo_target = env_value("CARGO_BUILD_TARGET")
    if cargo_target:
        return cargo_target, "CARGO_BUILD_TARGET"

    config_target, config_source = cargo_config_target(root)
    if config_target:
        return config_target, config_source or ".cargo/config"

    if rustc_host:
        return rustc_host, "rustc host"

    return None, "unknown"


def target_arch(target: str | None) -> str:
    if target is None:
        return "unknown"
    if target.startswith("x86_64"):
        return "x64"
    if target.startswith(("i586", "i686")):
        return "x86"
    if target.startswith("aarch64"):
        return "arm64"
    if target.startswith("arm"):
        return "arm"
    return platform.machine() or "unknown"


def is_windows_msvc_target(target: str | None) -> bool:
    return target is not None and "windows-msvc" in target


def first_existing(paths: Sequence[Path]) -> Path | None:
    for path in paths:
        if safe_exists(path):
            return path
    return None


def candidate_program_files_roots() -> list[Path]:
    roots: list[Path] = []
    for name in ("ProgramFiles(x86)", "ProgramFiles"):
        value = env_value(name)
        if value is not None:
            roots.append(Path(value))
    roots.extend([Path("C:/Program Files (x86)"), Path("C:/Program Files")])

    unique: list[Path] = []
    seen: set[str] = set()
    for root in roots:
        key = str(root).lower()
        if key not in seen:
            seen.add(key)
            unique.append(root)
    return unique


def link_patterns(toolset_dir: Path, arch: str) -> list[Path]:
    arches = [arch, "x64", "x86", "arm64", "arm"]
    hosts = ["Hostx64", "Hostx86", "Hostarm64"]
    paths: list[Path] = []
    for host in hosts:
        for candidate_arch in arches:
            paths.append(toolset_dir / "bin" / host / candidate_arch / "link.exe")
    return paths


def discover_visual_studio(arch: str) -> dict[str, Any]:
    env_vars = {
        name: env_value(name)
        for name in (
            "VSINSTALLDIR",
            "VCINSTALLDIR",
            "VCToolsInstallDir",
            "VisualStudioVersion",
        )
    }
    vs_roots = [
        root / "Microsoft Visual Studio"
        for root in candidate_program_files_roots()
        if safe_is_dir(root / "Microsoft Visual Studio")
    ]

    installations: list[dict[str, Any]] = []
    link_candidates: list[str] = []
    vcvarsall_candidates: list[str] = []
    vsdevcmd_candidates: list[str] = []

    env_toolset = env_value("VCToolsInstallDir")
    if env_toolset:
        for path in link_patterns(Path(env_toolset), arch):
            if safe_is_file(path):
                link_candidates.append(str(path))

    for vs_root in vs_roots:
        for major_dir in safe_iterdir(vs_root):
            if not safe_is_dir(major_dir):
                continue
            if major_dir.name.lower() in {"installer", "shared", "packages"}:
                continue

            for edition_dir in safe_iterdir(major_dir):
                if not safe_is_dir(edition_dir):
                    continue
                vc_root = edition_dir / "VC"
                common_tools = edition_dir / "Common7" / "Tools"
                if not safe_is_dir(vc_root) and not safe_is_dir(common_tools):
                    continue

                vcvarsall = vc_root / "Auxiliary" / "Build" / "vcvarsall.bat"
                vsdevcmd = common_tools / "VsDevCmd.bat"
                if safe_is_file(vcvarsall):
                    vcvarsall_candidates.append(str(vcvarsall))
                if safe_is_file(vsdevcmd):
                    vsdevcmd_candidates.append(str(vsdevcmd))

                toolset_root = vc_root / "Tools" / "MSVC"
                toolsets: list[str] = []
                if safe_is_dir(toolset_root):
                    for toolset_dir in safe_iterdir(toolset_root):
                        if not safe_is_dir(toolset_dir):
                            continue
                        toolsets.append(toolset_dir.name)
                        for link_candidate in link_patterns(toolset_dir, arch):
                            if safe_is_file(link_candidate):
                                link_candidates.append(str(link_candidate))

                installations.append(
                    {
                        "path": str(edition_dir),
                        "major": major_dir.name,
                        "edition": edition_dir.name,
                        "vcvarsall": str(vcvarsall) if safe_is_file(vcvarsall) else None,
                        "vsdevcmd": str(vsdevcmd) if safe_is_file(vsdevcmd) else None,
                        "msvc_toolsets": toolsets[:MAX_CANDIDATES],
                    }
                )

    vswhere = first_existing(
        [
            root
            / "Microsoft Visual Studio"
            / "Installer"
            / "vswhere.exe"
            for root in candidate_program_files_roots()
        ]
    )

    unique_link_candidates = sorted(dict.fromkeys(link_candidates))
    return {
        "environment": env_vars,
        "vswhere": path_to_str(vswhere),
        "installations": installations[:MAX_CANDIDATES],
        "vcvarsall_candidates": sorted(dict.fromkeys(vcvarsall_candidates))[
            :MAX_CANDIDATES
        ],
        "vsdevcmd_candidates": sorted(dict.fromkeys(vsdevcmd_candidates))[
            :MAX_CANDIDATES
        ],
        "link_candidates": unique_link_candidates[:MAX_CANDIDATES],
        "link_candidate_count": len(unique_link_candidates),
    }


def split_env_paths(value: str | None) -> list[Path]:
    if value is None:
        return []
    return [Path(item.strip('"')) for item in value.split(os.pathsep) if item.strip()]


def find_lib_in_paths(paths: Sequence[Path], lib_name: str) -> str | None:
    for directory in paths:
        candidate = directory / lib_name
        if safe_is_file(candidate):
            return str(candidate)
    return None


def sdk_roots() -> list[Path]:
    roots: list[Path] = []
    windows_sdk_dir = env_value("WindowsSdkDir")
    if windows_sdk_dir:
        roots.append(Path(windows_sdk_dir))
    for root in candidate_program_files_roots():
        roots.append(root / "Windows Kits" / "10")
        roots.append(root / "Windows Kits" / "8.1")

    unique: list[Path] = []
    seen: set[str] = set()
    for root in roots:
        key = str(root).lower()
        if key not in seen and safe_is_dir(root):
            seen.add(key)
            unique.append(root)
    return unique


def discover_windows_sdk(arch: str) -> dict[str, Any]:
    lib_env_paths = split_env_paths(env_value("LIB"))
    kernel32_from_env = find_lib_in_paths(lib_env_paths, "kernel32.lib")
    ucrt_from_env = find_lib_in_paths(lib_env_paths, "ucrt.lib")

    versions: list[dict[str, Any]] = []
    kernel32_from_sdk = None
    ucrt_from_sdk = None
    for root in sdk_roots():
        lib_root = root / "Lib"
        if not safe_is_dir(lib_root):
            continue
        for version_dir in sorted(safe_iterdir(lib_root), key=lambda item: item.name, reverse=True):
            if not safe_is_dir(version_dir):
                continue
            kernel32 = version_dir / "um" / arch / "kernel32.lib"
            ucrt = version_dir / "ucrt" / arch / "ucrt.lib"
            kernel32_found = safe_is_file(kernel32)
            ucrt_found = safe_is_file(ucrt)
            if kernel32_found and kernel32_from_sdk is None:
                kernel32_from_sdk = str(kernel32)
            if ucrt_found and ucrt_from_sdk is None:
                ucrt_from_sdk = str(ucrt)
            if kernel32_found or ucrt_found:
                versions.append(
                    {
                        "root": str(root),
                        "version": version_dir.name,
                        "arch": arch,
                        "kernel32_lib": str(kernel32) if kernel32_found else None,
                        "ucrt_lib": str(ucrt) if ucrt_found else None,
                    }
                )

    return {
        "environment": {
            "WindowsSdkDir": env_value("WindowsSdkDir"),
            "WindowsSDKLibVersion": env_value("WindowsSDKLibVersion"),
            "LIB_set": env_value("LIB") is not None,
            "LIB_path_count": len(lib_env_paths),
        },
        "kernel32_lib": kernel32_from_env or kernel32_from_sdk,
        "kernel32_lib_source": (
            "LIB" if kernel32_from_env else "Windows Kits" if kernel32_from_sdk else None
        ),
        "ucrt_lib": ucrt_from_env or ucrt_from_sdk,
        "ucrt_lib_source": (
            "LIB" if ucrt_from_env else "Windows Kits" if ucrt_from_sdk else None
        ),
        "detected_versions": versions[:MAX_CANDIDATES],
        "detected_version_count": len(versions),
    }


def blocker(blocker_id: str, message: str, evidence: Sequence[str], action: str) -> dict[str, Any]:
    return {
        "id": blocker_id,
        "message": message,
        "evidence": list(evidence),
        "operator_action": action,
    }


def warning(warning_id: str, message: str, evidence: Sequence[str]) -> dict[str, Any]:
    return {
        "id": warning_id,
        "message": message,
        "evidence": list(evidence),
    }


def build_report(args: argparse.Namespace) -> dict[str, Any]:
    rustc_path = shutil.which("rustc")
    rustc_verbose = run_command([rustc_path or "rustc", "-Vv"]) if rustc_path else None
    rustc_details = (
        parse_rustc_verbose(rustc_verbose["stdout"])
        if rustc_verbose and rustc_verbose["returncode"] == 0
        else {
            "version": None,
            "host": None,
            "release": None,
            "commit_hash": None,
            "llvm_version": None,
        }
    )

    target, target_source = selected_target(args.target, rustc_details["host"], ROOT)
    arch = target_arch(target)
    target_libdir = (
        run_command([rustc_path, "--print", "target-libdir", "--target", target])
        if rustc_path and target
        else None
    )

    link_path = shutil.which("link.exe")
    visual_studio = discover_visual_studio(arch)
    windows_sdk = discover_windows_sdk(arch)

    blockers: list[dict[str, Any]] = []
    warnings: list[dict[str, Any]] = []
    notes: list[str] = [
        "This preflight is read-only. It does not install, repair, or configure Visual Studio, Build Tools, Windows SDK, Rust, or PATH.",
        "Default mode reports blockers for evidence. Use --strict when a caller should fail on the blocker.",
    ]

    if rustc_path is None:
        blockers.append(
            blocker(
                "RUSTC_NOT_FOUND",
                "rustc is not available on PATH, so the Rust target cannot be verified.",
                ["shutil.which('rustc') returned no path"],
                "Install or expose the repository Rust toolchain, then rerun this preflight.",
            )
        )
    elif rustc_verbose is not None and rustc_verbose["returncode"] != 0:
        blockers.append(
            blocker(
                "RUSTC_VERSION_FAILED",
                "rustc -Vv did not complete successfully.",
                [
                    f"returncode={rustc_verbose['returncode']}",
                    f"stderr={rustc_verbose['stderr'] or '<empty>'}",
                ],
                "Inspect the Rust toolchain installation before running Rust gates.",
            )
        )

    if target is None:
        blockers.append(
            blocker(
                "RUST_TARGET_UNKNOWN",
                "No Rust target could be inferred from CLI, Cargo environment, Cargo config, or rustc host.",
                ["target_source=unknown"],
                "Pass --target explicitly or make rustc available so the host target can be read.",
            )
        )

    if target_libdir is not None and target_libdir["returncode"] != 0:
        blockers.append(
            blocker(
                "RUST_TARGET_LIBDIR_UNAVAILABLE",
                "rustc could not resolve the standard-library directory for the selected target.",
                [
                    f"target={target}",
                    f"returncode={target_libdir['returncode']}",
                    f"stderr={target_libdir['stderr'] or '<empty>'}",
                ],
                "Install the requested Rust target with rustup or select the target actually used by the gate.",
            )
        )

    windows_msvc = is_windows_msvc_target(target)
    if windows_msvc and link_path is None:
        evidence = ["link.exe is not on PATH"]
        if visual_studio["link_candidates"]:
            evidence.append(
                "candidate link.exe exists outside PATH: "
                f"{visual_studio['link_candidates'][0]}"
            )
        else:
            evidence.append("no Visual Studio MSVC link.exe candidate was detected")
        blockers.append(
            blocker(
                "MSVC_LINK_EXE_MISSING",
                "The selected Windows MSVC Rust target needs link.exe, but link.exe is not available on PATH.",
                evidence,
                "Run the gate from a Visual Studio Developer shell or expose the MSVC linker environment, then rerun this preflight.",
            )
        )

    if windows_msvc and windows_sdk["kernel32_lib"] is None:
        blockers.append(
            blocker(
                "WINDOWS_SDK_KERNEL32_LIB_MISSING",
                "kernel32.lib was not detected for the selected MSVC target architecture.",
                [
                    f"arch={arch}",
                    "checked LIB and known Windows Kits roots",
                ],
                "Install or expose a Windows SDK that contains UM libraries for the target architecture.",
            )
        )

    if windows_msvc and windows_sdk["ucrt_lib"] is None:
        blockers.append(
            blocker(
                "WINDOWS_SDK_UCRT_LIB_MISSING",
                "ucrt.lib was not detected for the selected MSVC target architecture.",
                [
                    f"arch={arch}",
                    "checked LIB and known Windows Kits roots",
                ],
                "Install or expose a Windows SDK that contains UCRT libraries for the target architecture.",
            )
        )

    if windows_msvc and not visual_studio["installations"]:
        warnings.append(
            warning(
                "VISUAL_STUDIO_INSTALLATION_NOT_DETECTED",
                "No Visual Studio installation with VC tooling was found under known Program Files roots.",
                [
                    "checked Microsoft Visual Studio directories under Program Files roots",
                    f"vswhere={visual_studio['vswhere'] or '<not found>'}",
                ],
            )
        )

    if windows_msvc and link_path is None and visual_studio["link_candidates"]:
        warnings.append(
            warning(
                "MSVC_LINK_EXISTS_OUTSIDE_PATH",
                "An MSVC link.exe candidate exists, but the current shell has not exposed it on PATH.",
                [visual_studio["link_candidates"][0]],
            )
        )

    if windows_msvc and windows_sdk["environment"]["LIB_set"] is False:
        warnings.append(
            warning(
                "LIB_ENV_NOT_SET",
                "The LIB environment variable is not set; a plain shell may still fail to link even when SDK files are installed.",
                ["LIB is required evidence for many MSVC developer-shell setups"],
            )
        )

    if blockers:
        result = "blocked"
    elif windows_msvc:
        result = "passed"
    elif target is None:
        result = "inconclusive"
    else:
        result = "not_applicable"

    return {
        "schema_version": 1,
        "result": result,
        "strict": bool(args.strict),
        "repository": str(ROOT),
        "platform": {
            "system": platform.system(),
            "release": platform.release(),
            "machine": platform.machine(),
            "python": sys.version.split()[0],
        },
        "rust": {
            "rustc_path": rustc_path,
            "rustc_verbose": rustc_verbose,
            "version": rustc_details["version"],
            "host": rustc_details["host"],
            "target": target,
            "target_source": target_source,
            "target_arch": arch,
            "target_is_windows_msvc": windows_msvc,
            "target_libdir": (
                target_libdir["stdout"]
                if target_libdir and target_libdir["returncode"] == 0
                else None
            ),
            "target_libdir_probe": target_libdir,
        },
        "msvc": {
            "link_exe_on_path": link_path is not None,
            "link_exe_path": link_path,
            "visual_studio": visual_studio,
            "windows_sdk": windows_sdk,
        },
        "blockers": blockers,
        "warnings": warnings,
        "notes": notes,
    }


def print_text(report: dict[str, Any]) -> None:
    rust = report["rust"]
    msvc = report["msvc"]

    print("Andromeda Windows/MSVC Preflight")
    print(f"Repository: {report['repository']}")
    print(f"Result: {report['result']}")
    print(
        "Target: "
        f"{rust['target'] or '<unknown>'} "
        f"(source: {rust['target_source']}, arch: {rust['target_arch']})"
    )
    print(f"rustc: {rust['version'] or '<unavailable>'}")
    print(f"rustc path: {rust['rustc_path'] or '<not on PATH>'}")
    print(f"target libdir: {rust['target_libdir'] or '<unavailable>'}")
    print()

    print("MSVC Linker")
    print(f"  link.exe on PATH: {'yes' if msvc['link_exe_on_path'] else 'no'}")
    print(f"  link.exe path: {msvc['link_exe_path'] or '<not on PATH>'}")
    candidates = msvc["visual_studio"]["link_candidates"]
    print(f"  MSVC link.exe candidates: {msvc['visual_studio']['link_candidate_count']}")
    for candidate in candidates[:5]:
        print(f"    {candidate}")
    print()

    print("Visual Studio Build Tools Indicators")
    visual_studio = msvc["visual_studio"]
    print(f"  vswhere: {visual_studio['vswhere'] or '<not found>'}")
    print(f"  installations: {len(visual_studio['installations'])}")
    for installation in visual_studio["installations"][:5]:
        toolsets = ", ".join(installation["msvc_toolsets"]) or "<none>"
        print(f"    {installation['path']} (toolsets: {toolsets})")
    print("  environment:")
    for name, value in visual_studio["environment"].items():
        print(f"    {name}: {value or '<unset>'}")
    print()

    print("Windows SDK Libraries")
    sdk = msvc["windows_sdk"]
    print(f"  kernel32.lib: {sdk['kernel32_lib'] or '<not detected>'}")
    print(f"  ucrt.lib: {sdk['ucrt_lib'] or '<not detected>'}")
    print(f"  detected SDK library versions: {sdk['detected_version_count']}")
    print("  environment:")
    for name, value in sdk["environment"].items():
        print(f"    {name}: {value if value not in (None, '') else '<unset>'}")
    print()

    if report["blockers"]:
        print("Blockers")
        for item in report["blockers"]:
            print(f"  - {item['id']}: {item['message']}")
            for evidence in item["evidence"]:
                print(f"    evidence: {evidence}")
            print(f"    operator action: {item['operator_action']}")
        print()
    else:
        print("Blockers: none")
        print()

    if report["warnings"]:
        print("Warnings")
        for item in report["warnings"]:
            print(f"  - {item['id']}: {item['message']}")
            for evidence in item["evidence"]:
                print(f"    evidence: {evidence}")
        print()

    print("Notes")
    for note in report["notes"]:
        print(f"  - {note}")


def main(argv: Sequence[str]) -> int:
    args = parse_args(argv)
    report = build_report(args)

    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print_text(report)

    if args.strict and report["blockers"]:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
