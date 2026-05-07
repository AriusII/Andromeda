#!/usr/bin/env python3
"""Classified Andromeda project policy gate.

The gate blocks active runtime/API drift while allowing doctrine-negative
references in documentation, tests, skills, comments, and guardrail code. It is
intentionally narrower than a repository-wide text grep: blocking scope is
limited to Cargo manifests, active Protobuf schemas, active Rust runtime
sources, active SRPL source artifacts, and GPU exclusion on critical
commit/WAL/recovery/MVCC/security paths.
"""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Iterator, Sequence


EXCLUDED_DIRS = {
    ".git",
    ".mypy_cache",
    ".pytest_cache",
    "__pycache__",
    "dist",
    "node_modules",
    "target",
}

NON_BLOCKING_REFERENCE_DIRS = {
    ".agents",
    ".codex",
    "docs",
    "instructions",
    "registries",
    "workflows",
}

TEST_DIRS = {
    "benches",
    "examples",
    "fixtures",
    "testdata",
    "tests",
}

GITHUB_NON_BLOCKING_DIRS = {
    "actions",
    "docs",
    "instructions",
    "scripts",
    "workflows",
}

DOCUMENTATION_EXTS = {".md", ".txt", ".yaml", ".yml"}
TEXT_EXTS = {".proto", ".rs", ".srpl", ".toml"}


@dataclass(frozen=True)
class PolicyViolation:
    path: Path
    line: int
    code: str
    message: str
    scope: str
    excerpt: str


@dataclass(frozen=True)
class LinePolicy:
    code: str
    pattern: re.Pattern[str]
    message: str


MANIFEST_POLICIES: tuple[LinePolicy, ...] = (
    LinePolicy(
        "grpc_dependency",
        re.compile(
            r'(?:^\s*(?:grpc|grpcio|grpcio-sys|grpc-web|prost-grpc|tower-grpc)\s*='
            r'|^\s*name\s*=\s*"(?:grpc|grpcio|grpcio-sys|grpc-web|prost-grpc|tower-grpc)"'
            r'|^\s*package\s*=\s*"(?:grpc|grpcio|grpcio-sys|grpc-web|prost-grpc|tower-grpc)")',
            re.IGNORECASE,
        ),
        "gRPC dependencies are forbidden. Use QUIC + custom typed RPC.",
    ),
    LinePolicy(
        "tonic_dependency",
        re.compile(
            r'(?:^\s*(?:tonic|tonic-build|tonic-health|tonic-reflection|tonic-web)\s*='
            r'|^\s*name\s*=\s*"(?:tonic|tonic-build|tonic-health|tonic-reflection|tonic-web)"'
            r'|^\s*package\s*=\s*"(?:tonic|tonic-build|tonic-health|tonic-reflection|tonic-web)")',
            re.IGNORECASE,
        ),
        "tonic dependencies imply gRPC and are forbidden.",
    ),
    LinePolicy(
        "json_runtime_dependency",
        re.compile(
            r'(?:^\s*(?:serde_json|jsonrpsee)\s*='
            r'|^\s*name\s*=\s*"(?:serde_json|jsonrpsee)"'
            r'|^\s*package\s*=\s*"(?:serde_json|jsonrpsee)")',
            re.IGNORECASE,
        ),
        "Runtime JSON dependencies are forbidden on Andromeda protocol/result surfaces.",
    ),
    LinePolicy(
        "sql_dependency",
        re.compile(
            r'(?:^\s*(?:diesel|mysql|postgres|rusqlite|sqlx|tokio-postgres)\s*='
            r'|^\s*name\s*=\s*"(?:diesel|mysql|postgres|rusqlite|sqlx|tokio-postgres)"'
            r'|^\s*package\s*=\s*"(?:diesel|mysql|postgres|rusqlite|sqlx|tokio-postgres)")',
            re.IGNORECASE,
        ),
        "Ad hoc SQL dependencies are forbidden on the application/runtime surface.",
    ),
)

PROTO_POLICIES: tuple[LinePolicy, ...] = (
    LinePolicy(
        "protobuf_service",
        re.compile(r"^\s*service\s+[A-Za-z_][A-Za-z0-9_]*\b", re.IGNORECASE),
        "Protobuf service declarations are forbidden; schemas must be message-only.",
    ),
    LinePolicy(
        "protobuf_rpc",
        re.compile(r"^\s*rpc\s+[A-Za-z_][A-Za-z0-9_]*\s*\(", re.IGNORECASE),
        "Protobuf rpc declarations are forbidden; Andromeda uses custom typed RPC over QUIC.",
    ),
    LinePolicy(
        "grpc_reference",
        re.compile(r"\bgrpc\b", re.IGNORECASE),
        "gRPC references are forbidden on active protocol schemas.",
    ),
    LinePolicy(
        "tonic_reference",
        re.compile(r"\btonic\b", re.IGNORECASE),
        "tonic references are forbidden on active protocol schemas.",
    ),
    LinePolicy(
        "google_api_annotation",
        re.compile(r"\bgoogle\s*\.\s*api\b|google/api", re.IGNORECASE),
        "google.api annotations imply HTTP/gRPC transcoding and are forbidden.",
    ),
    LinePolicy(
        "json_wire",
        re.compile(
            r"\b(?:google\s*\.\s*protobuf\s*\.\s*(?:Struct|Value|ListValue|NullValue)"
            r"|[A-Za-z0-9_]*json[A-Za-z0-9_]*)\b",
            re.IGNORECASE,
        ),
        "Runtime JSON wire fields are forbidden; use typed Protobuf fields.",
    ),
    LinePolicy(
        "sql_surface",
        re.compile(
            r"\b(?:ad_hoc_sql|dynamic_sql|execute_sql|prepare_sql|raw_sql|"
            r"sql_text|statement_text|query_text|"
            r"[A-Za-z_][A-Za-z0-9_]*Query[A-Za-z0-9_]*|"
            r"[A-Za-z_][A-Za-z0-9_]*Sql[A-Za-z0-9_]*)\b"
        ),
        "Application-facing SQL/query terms are forbidden; use cataloged Procedure contracts.",
    ),
    LinePolicy(
        "srpl_unbounded_loop_surface",
        re.compile(
            r"\b(?:for_each|foreach|loop|repeat_until|repeat|while)(?:\b|_)",
            re.IGNORECASE,
        ),
        "Unbounded SRPL loop vocabulary is forbidden on active protocol schemas.",
    ),
)

RUST_SOURCE_POLICIES: tuple[LinePolicy, ...] = (
    LinePolicy(
        "tonic_binding",
        re.compile(r"\btonic(?:_build)?::", re.IGNORECASE),
        "tonic bindings are forbidden; Andromeda uses QUIC + custom typed RPC.",
    ),
    LinePolicy(
        "grpc_binding",
        re.compile(r"\b(?:grpc|grpcio|prost_grpc|tower_grpc)::", re.IGNORECASE),
        "gRPC bindings are forbidden; Andromeda uses QUIC + custom typed RPC.",
    ),
    LinePolicy(
        "json_runtime_wire",
        re.compile(r"\b(?:serde_json|jsonrpsee)::|\bserde_json\b|\bjsonrpsee\b"),
        "Runtime JSON wire handling is forbidden on active runtime/API sources.",
    ),
    LinePolicy(
        "json_runtime_default",
        re.compile(
            r'\b(?:DefaultJson|JsonDefault|PayloadEncoding::Json|WireFormat::Json|'
            r'default_json|json_default)\b|["\']application/json["\']',
            re.IGNORECASE,
        ),
        "Runtime JSON defaults are forbidden; use typed Protobuf/binary ResultStream payloads.",
    ),
    LinePolicy(
        "sql_runtime_dependency",
        re.compile(
            r"\b(?:diesel|mysql|postgres|rusqlite|sqlx|tokio_postgres)::",
            re.IGNORECASE,
        ),
        "Ad hoc SQL client/runtime APIs are forbidden.",
    ),
    LinePolicy(
        "sql_select_star",
        re.compile(r"\bSELECT\s+\*", re.IGNORECASE),
        "SELECT * is forbidden on active runtime/API source paths.",
    ),
    LinePolicy(
        "sql_text_surface",
        re.compile(
            r"\b(?:ad_hoc_sql|dynamic_sql|execute_sql|prepare_sql|raw_sql|"
            r"sql_text|statement_text|query_text)\b",
            re.IGNORECASE,
        ),
        "Generic SQL text surfaces are forbidden; route execution through cataloged Procedures.",
    ),
    LinePolicy(
        "sql_dml_string",
        re.compile(
            r"""(?x)
            (?:r\#*)?["']\s*
            (?:
                SELECT\s+.+\bFROM\b
                |INSERT\s+INTO\b
                |UPDATE\s+[A-Za-z_][A-Za-z0-9_.]*\s+SET\b
                |DELETE\s+FROM\b
                |CREATE\s+TABLE\b
                |DROP\s+TABLE\b
            )
            """,
            re.IGNORECASE,
        ),
        "SQL DML/DDL text is forbidden on active runtime/API source paths.",
    ),
)

GPU_CRITICAL_PATH_POLICIES: tuple[LinePolicy, ...] = (
    LinePolicy(
        "gpu_critical_path",
        re.compile(
            r"\b(?:gpu|Gpu|GPU|cuda|cudarc|wgpu|opencl|OpenCL|rocm|ROCm|vulkan|Vulkan|metal)\b"
        ),
        "GPU references are forbidden in commit/WAL/recovery/MVCC/security critical-path sources.",
    ),
)

SRPL_SOURCE_POLICIES: tuple[LinePolicy, ...] = (
    LinePolicy(
        "srpl_unbounded_loop_keyword",
        re.compile(
            r"\b(?:for\s+each|foreach|loop|repeat\s+until|repeat|while)\b",
            re.IGNORECASE,
        ),
        "Unbounded SRPL loop keywords are forbidden; use bounded relational operations.",
    ),
)


def rel_parts(path: Path, root: Path) -> tuple[str, ...]:
    return tuple(path.relative_to(root).parts)


def has_part(path: Path, root: Path, candidates: set[str]) -> bool:
    return any(part in candidates for part in rel_parts(path, root))


def is_under_github_non_blocking(path: Path, root: Path) -> bool:
    parts = rel_parts(path, root)
    return len(parts) >= 2 and parts[0] == ".github" and parts[1] in GITHUB_NON_BLOCKING_DIRS


def is_test_path(path: Path, root: Path) -> bool:
    parts = rel_parts(path, root)
    return any(part in TEST_DIRS or part.startswith("test_") for part in parts)


def is_excluded(path: Path, root: Path) -> bool:
    if path.suffix not in TEXT_EXTS and path.name != "Cargo.toml":
        return True
    return has_part(path, root, EXCLUDED_DIRS)


def has_prefix(parts: tuple[str, ...], prefix: tuple[str, ...]) -> bool:
    return parts[: len(prefix)] == prefix


def is_gpu_critical_rust_path(path: Path, root: Path) -> bool:
    if path.suffix != ".rs":
        return False

    parts = rel_parts(path, root)
    lower_parts = tuple(part.lower() for part in parts)
    name = path.name.lower()
    stem = path.stem.lower()

    if has_prefix(lower_parts, ("crates", "andromeda-tx", "src")):
        return True

    if has_prefix(lower_parts, ("crates", "andromeda-storage", "src")):
        if len(lower_parts) >= 4 and lower_parts[3] in {
            "file_wal",
            "recovery",
            "wal_codec",
            "write_ahead_log",
        }:
            return True
        if name in {"restore_orchestration.rs"}:
            return True
        if stem.startswith("wal_") or stem.endswith("_wal") or "_wal_" in stem:
            return True

    if has_prefix(lower_parts, ("crates", "andromeda-core", "src", "principal")):
        return True

    if has_prefix(
        lower_parts,
        ("crates", "andromeda-observe", "src", "events", "durable_audit"),
    ):
        return True

    return False


def policy_scope(path: Path, root: Path) -> str | None:
    parts = rel_parts(path, root)

    if is_excluded(path, root):
        return None
    if has_part(path, root, NON_BLOCKING_REFERENCE_DIRS):
        return None
    if is_under_github_non_blocking(path, root):
        return None
    if is_test_path(path, root):
        return None
    if path.suffix in DOCUMENTATION_EXTS:
        return None

    if path.name == "Cargo.toml":
        return "dependency-manifest"
    if path.suffix == ".proto":
        return "protocol-schema"
    if path.suffix == ".srpl":
        return "srpl-source-artifact"
    if (
        path.suffix == ".rs"
        and "crates" in parts
        and ("src" in parts or path.name == "build.rs")
    ):
        return "runtime-rust-source"
    return None


def iter_manifest_lines(text: str) -> Iterator[tuple[int, str]]:
    for line_number, line in enumerate(text.splitlines(), start=1):
        code = line.split("#", 1)[0].rstrip()
        if code.strip():
            yield line_number, code


def strip_line_comments(line: str, marker: str) -> str:
    return line.split(marker, 1)[0].rstrip()


def iter_non_comment_lines(text: str, line_comment: str) -> Iterator[tuple[int, str]]:
    in_block = False
    for line_number, line in enumerate(text.splitlines(), start=1):
        cursor = line
        if in_block:
            if "*/" not in cursor:
                continue
            cursor = cursor.split("*/", 1)[1]
            in_block = False
        while "/*" in cursor:
            before, after = cursor.split("/*", 1)
            if "*/" in after:
                cursor = before + after.split("*/", 1)[1]
                continue
            cursor = before
            in_block = True
            break
        code = strip_line_comments(cursor, line_comment)
        if code.strip():
            yield line_number, code


def brace_delta(line: str) -> int:
    return line.count("{") - line.count("}")


def iter_rust_policy_lines(text: str) -> Iterator[tuple[int, str]]:
    pending_cfg_test = False
    pending_test_fn = False
    skip_depth: int | None = None

    for line_number, line in iter_non_comment_lines(text, "//"):
        stripped = line.strip()

        if skip_depth is not None:
            skip_depth += brace_delta(line)
            if skip_depth <= 0:
                skip_depth = None
            continue

        if re.match(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]", stripped):
            pending_cfg_test = True
            continue
        if re.match(r"#\s*\[\s*test\s*\]", stripped):
            pending_test_fn = True
            continue

        if pending_cfg_test and re.search(r"\bmod\s+[A-Za-z_][A-Za-z0-9_]*\s*\{", stripped):
            skip_depth = max(brace_delta(line), 1)
            pending_cfg_test = False
            continue
        if pending_test_fn and re.search(r"\bfn\s+[A-Za-z_][A-Za-z0-9_]*\s*\(", stripped):
            skip_depth = max(brace_delta(line), 1)
            pending_test_fn = False
            continue

        pending_cfg_test = False
        pending_test_fn = False
        yield line_number, line


def scan_policy_lines(
    path: Path,
    scope: str,
    lines: Iterable[tuple[int, str]],
    policies: Sequence[LinePolicy],
) -> list[PolicyViolation]:
    violations: list[PolicyViolation] = []
    for line_number, line in lines:
        for policy in policies:
            if policy.pattern.search(line):
                violations.append(
                    PolicyViolation(
                        path=path,
                        line=line_number,
                        code=policy.code,
                        message=policy.message,
                        scope=scope,
                        excerpt=line.strip()[:240],
                    )
                )
    return violations


def scan_file(path: Path, root: Path) -> list[PolicyViolation]:
    scope = policy_scope(path, root)
    if scope is None:
        return []

    text = path.read_text(encoding="utf-8", errors="ignore")
    if scope == "dependency-manifest":
        return scan_policy_lines(path, scope, iter_manifest_lines(text), MANIFEST_POLICIES)
    if scope == "protocol-schema":
        return scan_policy_lines(path, scope, iter_non_comment_lines(text, "//"), PROTO_POLICIES)
    if scope == "srpl-source-artifact":
        return scan_policy_lines(path, scope, iter_non_comment_lines(text, "--"), SRPL_SOURCE_POLICIES)
    if scope == "runtime-rust-source":
        violations = scan_policy_lines(
            path, scope, iter_rust_policy_lines(text), RUST_SOURCE_POLICIES
        )
        if is_gpu_critical_rust_path(path, root):
            violations.extend(
                scan_policy_lines(
                    path,
                    "gpu-critical-rust-source",
                    iter_rust_policy_lines(text),
                    GPU_CRITICAL_PATH_POLICIES,
                )
            )
        return violations
    return []


def run_policy_gate(root: Path | str = Path.cwd()) -> list[PolicyViolation]:
    root = Path(root).resolve()
    violations: list[PolicyViolation] = []
    for path in sorted(root.rglob("*")):
        if path.is_file():
            violations.extend(scan_file(path, root))
    return violations


def annotation_escape(value: object) -> str:
    text = str(value)
    return text.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def emit_violations(violations: Sequence[PolicyViolation]) -> None:
    for violation in violations:
        print(
            "::error "
            f"file={annotation_escape(violation.path)},"
            f"line={violation.line},"
            f"title={annotation_escape(violation.code)}::"
            f"{annotation_escape(violation.message)} "
            f"[scope={annotation_escape(violation.scope)}] "
            f"{annotation_escape(violation.excerpt)}"
        )


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path.cwd(),
        help="Repository root to scan. Defaults to the current working directory.",
    )
    args = parser.parse_args(argv)

    violations = run_policy_gate(args.root)
    if violations:
        emit_violations(violations)
        return 1

    print("Andromeda policy gate passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
