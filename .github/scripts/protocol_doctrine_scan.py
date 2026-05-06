#!/usr/bin/env python3
"""Protocol doctrine scanner for Andromeda protobuf contracts."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Sequence


DEFAULT_PROTO_ROOTS = (
    Path("crates/andromeda-proto/proto"),
    Path("proto"),
    Path("schemas/proto"),
)

DEFAULT_POLICY_ROOTS = (
    Path("Cargo.toml"),
    Path("Cargo.lock"),
    Path("crates/andromeda-proto/Cargo.toml"),
    Path("crates/andromeda-quic/Cargo.toml"),
    Path("crates/andromeda-exec/Cargo.toml"),
    Path("crates/andromeda-proto/src"),
    Path("crates/andromeda-quic/src"),
    Path("crates/andromeda-exec/src"),
)

DOCTRINE_ALLOW = "DOCTRINE_ALLOW"

LINE_POLICIES: tuple[tuple[str, re.Pattern[str], str], ...] = (
    (
        "protobuf_service",
        re.compile(r"^\s*service\s+[A-Za-z_][A-Za-z0-9_]*\b", re.IGNORECASE),
        "Protobuf service declarations are forbidden; Andromeda uses QUIC + custom RPC.",
    ),
    (
        "protobuf_rpc",
        re.compile(r"^\s*rpc\s+[A-Za-z_][A-Za-z0-9_]*\s*\(", re.IGNORECASE),
        "Protobuf rpc method declarations are forbidden; schemas must remain message-only.",
    ),
    (
        "grpc_reference",
        re.compile(r"\bgrpc\b", re.IGNORECASE),
        "gRPC references are forbidden on the protocol schema surface.",
    ),
    (
        "tonic_reference",
        re.compile(r"\btonic\b", re.IGNORECASE),
        "tonic/gRPC bindings are forbidden on the protocol schema surface.",
    ),
    (
        "google_api_annotation",
        re.compile(r"google\s*\.\s*api|google/api", re.IGNORECASE),
        "google.api annotations imply HTTP/gRPC transcoding and are forbidden.",
    ),
    (
        "json_wire",
        re.compile(
            r"\b(?=[A-Za-z_][A-Za-z0-9_]*\b)[A-Za-z0-9_]*json[A-Za-z0-9_]*\b",
            re.IGNORECASE,
        ),
        "JSON wire usage is forbidden on runtime protobuf contracts.",
    ),
    (
        "json_wire",
        re.compile(
            r"\bgoogle\s*\.\s*protobuf\s*\.\s*(?:Struct|Value|ListValue|NullValue)\b"
        ),
        "Dynamic protobuf JSON value types are forbidden on runtime contracts.",
    ),
    (
        "sql_query_surface",
        re.compile(
            r"\b(?:"
            r"[A-Za-z_][A-Za-z0-9_]*(?:Query|query)[A-Za-z0-9_]*"
            r"|[A-Za-z_][A-Za-z0-9_]*(?:SQL|Sql|sql)[A-Za-z0-9_]*"
            r"|[A-Za-z_][A-Za-z0-9_]*View[A-Za-z0-9_]*"
            r"|view(?:_[A-Za-z0-9_]+)*"
            r"|[A-Za-z_][A-Za-z0-9_]*_view(?:_[A-Za-z0-9_]+)*"
            r"|materialized_view(?:_[A-Za-z0-9_]+)*"
            r")\b"
        ),
        "Application-facing SQL/query/view terminology is forbidden; use Procedure, Invocation, Map, and StructuredObject contract names.",
    ),
)

DEPENDENCY_POLICIES: tuple[tuple[str, re.Pattern[str], str], ...] = (
    (
        "tonic_reference",
        re.compile(
            r'(?:^\s*(?:\[.*\b)?(?:tonic|tonic-build|tonic-health|tonic-web)\b'
            r'|^\s*name\s*=\s*"(?:tonic|tonic-build|tonic-health|tonic-web)\b'
            r'|\bpackage\s*=\s*"(?:tonic|tonic-build|tonic-health|tonic-web)\b)',
            re.IGNORECASE,
        ),
        "tonic dependencies are forbidden; Andromeda uses QUIC + custom RPC.",
    ),
    (
        "grpc_reference",
        re.compile(
            r'(?:^\s*(?:\[.*\b)?(?:grpc|grpcio|grpcio-sys|tower-grpc)\b'
            r'|^\s*name\s*=\s*"(?:grpc|grpcio|grpcio-sys|tower-grpc)\b'
            r'|\bpackage\s*=\s*"(?:grpc|grpcio|grpcio-sys|tower-grpc)\b)',
            re.IGNORECASE,
        ),
        "gRPC dependencies are forbidden; Andromeda uses QUIC + custom RPC.",
    ),
)

PROTOCOL_SURFACE_DEPENDENCY_POLICIES: tuple[
    tuple[str, re.Pattern[str], str], ...
] = (
    (
        "json_wire",
        re.compile(
            r'(?:^\s*(?:\[.*\b)?(?:serde_json|jsonrpsee)\b'
            r'|^\s*name\s*=\s*"(?:serde_json|jsonrpsee)\b'
            r'|\bpackage\s*=\s*"(?:serde_json|jsonrpsee)\b)',
            re.IGNORECASE,
        ),
        "JSON runtime dependencies are forbidden on the protocol/result surface.",
    ),
)

SOURCE_POLICIES: tuple[tuple[str, re.Pattern[str], str], ...] = (
    (
        "tonic_reference",
        re.compile(r"\btonic(?:_build)?::", re.IGNORECASE),
        "tonic bindings are forbidden; Andromeda uses QUIC + custom RPC.",
    ),
    (
        "grpc_reference",
        re.compile(r"\b(?:grpc|grpcio)::", re.IGNORECASE),
        "gRPC bindings are forbidden; Andromeda uses QUIC + custom RPC.",
    ),
    (
        "json_wire",
        re.compile(r"\bserde_json::|\bserde_json\b", re.IGNORECASE),
        "JSON runtime usage is forbidden on the protocol/result surface.",
    ),
)


@dataclass(frozen=True)
class Violation:
    path: str
    line: int
    code: str
    message: str
    excerpt: str = ""


@dataclass(frozen=True)
class ExpectedEnum:
    path_suffix: str
    enum_name: str
    values: dict[str, int]
    reserved: tuple[str, ...] = ()


@dataclass(frozen=True)
class EnumBlock:
    path: Path
    name: str
    line: int
    values: dict[str, int]
    reserved: tuple[str, ...]


EXPECTED_ENUMS: tuple[ExpectedEnum, ...] = (
    ExpectedEnum(
        "andromeda/protocol/v1/envelope.proto",
        "PayloadKind",
        {
            "PAYLOAD_KIND_UNSPECIFIED": 0,
            "PAYLOAD_KIND_HELLO": 1,
            "PAYLOAD_KIND_AUTH": 2,
            "PAYLOAD_KIND_CONTRACT_REQUEST": 3,
            "PAYLOAD_KIND_CONTRACT_RESPONSE": 4,
            "PAYLOAD_KIND_RPC_EXECUTE_REQUEST": 5,
            "PAYLOAD_KIND_RPC_METADATA": 6,
            "PAYLOAD_KIND_RPC_BATCH": 7,
            "PAYLOAD_KIND_RPC_COMPLETION": 8,
            "PAYLOAD_KIND_ERROR": 9,
        },
        ("reserved 10 to 99;", "reserved 100 to 199;"),
    ),
    ExpectedEnum(
        "andromeda/protocol/v1/completion.proto",
        "Status",
        {
            "STATUS_UNSPECIFIED": 0,
            "STATUS_COMMITTED": 1,
            "STATUS_ROLLED_BACK": 2,
            "STATUS_FAILED_BEFORE_TRANSACTION": 3,
            "STATUS_CANCELLED": 4,
            "STATUS_POISONED": 5,
            "STATUS_PERMISSION_DENIED": 6,
            "STATUS_CONTRACT_REJECTED": 7,
            "STATUS_SYSTEM_UNAVAILABLE": 8,
        },
    ),
    ExpectedEnum(
        "andromeda/protocol/v1/completion.proto",
        "TransactionOutcome",
        {
            "TRANSACTION_OUTCOME_UNSPECIFIED": 0,
            "TRANSACTION_OUTCOME_NOT_STARTED": 1,
            "TRANSACTION_OUTCOME_COMMITTED": 2,
            "TRANSACTION_OUTCOME_ROLLED_BACK": 3,
            "TRANSACTION_OUTCOME_FAILED": 4,
            "TRANSACTION_OUTCOME_CANCELLED": 5,
        },
    ),
    ExpectedEnum(
        "andromeda/contract/v1/catalog.proto",
        "Status",
        {
            "STATUS_UNSPECIFIED": 0,
            "STATUS_RESOLVED": 1,
            "STATUS_NOT_FOUND": 2,
            "STATUS_CATALOG_VERSION_MISMATCH": 3,
            "STATUS_CONTRACT_HASH_MISMATCH": 4,
            "STATUS_NOT_SOURCE_GENERATOR_READY": 5,
            "STATUS_PERMISSION_DENIED": 6,
            "STATUS_UNSUPPORTED": 7,
            "STATUS_MALFORMED": 8,
            "STATUS_INTERNAL": 9,
            "STATUS_CATALOG_NOT_READY": 10,
            "STATUS_AUTH_REQUIRED": 11,
        },
    ),
)


def discover_proto_files(roots: Sequence[Path]) -> list[Path]:
    files: list[Path] = []
    for root in roots:
        if root.is_file() and root.suffix == ".proto":
            files.append(root)
        elif root.is_dir():
            files.extend(root.rglob("*.proto"))
    return sorted(set(files), key=lambda path: path.as_posix())


def discover_policy_files(roots: Sequence[Path]) -> list[Path]:
    files: list[Path] = []
    for root in roots:
        if root.is_file() and is_governed_policy_file(root):
            files.append(root)
        elif root.is_dir():
            files.extend(path for path in root.rglob("*") if is_governed_policy_file(path))
    return sorted(set(files), key=lambda path: path.as_posix())


def is_governed_policy_file(path: Path) -> bool:
    if not path.is_file():
        return False
    return path.name in {"Cargo.toml", "Cargo.lock"} or path.suffix == ".rs"


def strip_comments_preserve_lines(text: str) -> str:
    active: list[str] = []
    index = 0
    in_block_comment = False

    while index < len(text):
        character = text[index]
        next_character = text[index + 1] if index + 1 < len(text) else ""

        if in_block_comment:
            if character == "*" and next_character == "/":
                in_block_comment = False
                index += 2
                continue
            if character == "\n":
                active.append("\n")
            index += 1
            continue

        if character == "/" and next_character == "/":
            index += 2
            while index < len(text) and text[index] != "\n":
                index += 1
            if index < len(text):
                active.append("\n")
                index += 1
            continue

        if character == "/" and next_character == "*":
            in_block_comment = True
            index += 2
            continue

        active.append(character)
        index += 1

    return "".join(active)


def strip_hash_comments_preserve_lines(text: str) -> str:
    lines: list[str] = []
    for line in text.splitlines(keepends=True):
        comment_index = line.find("#")
        if comment_index >= 0:
            newline = "\n" if line.endswith("\n") else ""
            lines.append(line[:comment_index] + newline)
        else:
            lines.append(line)
    return "".join(lines)


def strip_rust_comments_and_strings_preserve_lines(text: str) -> str:
    active: list[str] = []
    index = 0
    in_block_comment = False
    in_line_comment = False
    in_string = False
    in_raw_string_hashes: int | None = None

    while index < len(text):
        character = text[index]
        next_character = text[index + 1] if index + 1 < len(text) else ""

        if in_line_comment:
            if character == "\n":
                in_line_comment = False
                active.append("\n")
            else:
                active.append(" ")
            index += 1
            continue

        if in_block_comment:
            if character == "*" and next_character == "/":
                in_block_comment = False
                active.extend("  ")
                index += 2
                continue
            active.append("\n" if character == "\n" else " ")
            index += 1
            continue

        if in_raw_string_hashes is not None:
            if character == '"' and text[index + 1 : index + 1 + in_raw_string_hashes] == (
                "#" * in_raw_string_hashes
            ):
                active.append(" ")
                active.extend(" " * in_raw_string_hashes)
                index += 1 + in_raw_string_hashes
                in_raw_string_hashes = None
                continue
            active.append("\n" if character == "\n" else " ")
            index += 1
            continue

        if in_string:
            if character == "\\":
                active.append(" ")
                if index + 1 < len(text):
                    active.append("\n" if next_character == "\n" else " ")
                    index += 2
                else:
                    index += 1
                continue
            if character == '"':
                in_string = False
            active.append("\n" if character == "\n" else " ")
            index += 1
            continue

        raw_string_match = re.match(r'r(#+)?"', text[index:])
        if raw_string_match:
            in_raw_string_hashes = len(raw_string_match.group(1) or "")
            active.extend(" " * raw_string_match.end())
            index += raw_string_match.end()
            continue

        if character == "/" and next_character == "/":
            in_line_comment = True
            active.extend("  ")
            index += 2
            continue

        if character == "/" and next_character == "*":
            in_block_comment = True
            active.extend("  ")
            index += 2
            continue

        if character == '"':
            in_string = True
            active.append(" ")
            index += 1
            continue

        active.append(character)
        index += 1

    return "".join(active)


def scan_proto_file(path: Path) -> list[Violation]:
    raw = path.read_text(encoding="utf-8", errors="replace")
    active = strip_comments_preserve_lines(raw)
    raw_lines = raw.splitlines()
    violations: list[Violation] = []

    for line_number, line in enumerate(active.splitlines(), start=1):
        raw_line = raw_lines[line_number - 1] if line_number <= len(raw_lines) else ""
        if DOCTRINE_ALLOW in raw_line:
            continue

        for code, pattern, message in LINE_POLICIES:
            if pattern.search(line):
                violations.append(
                    Violation(
                        path=normalized_path(path),
                        line=line_number,
                        code=code,
                        message=message,
                        excerpt=line.strip(),
                    )
                )

    return violations


def scan_policy_file(path: Path) -> list[Violation]:
    raw = path.read_text(encoding="utf-8", errors="replace")
    if path.name in {"Cargo.toml", "Cargo.lock"}:
        active = strip_hash_comments_preserve_lines(raw)
        policies = list(DEPENDENCY_POLICIES)
        if path.name != "Cargo.lock" and is_protocol_surface_path(path):
            policies.extend(PROTOCOL_SURFACE_DEPENDENCY_POLICIES)
    elif path.suffix == ".rs" and is_protocol_surface_path(path):
        active = strip_rust_comments_and_strings_preserve_lines(raw)
        policies = list(SOURCE_POLICIES)
    else:
        return []

    raw_lines = raw.splitlines()
    violations: list[Violation] = []
    for line_number, line in enumerate(active.splitlines(), start=1):
        raw_line = raw_lines[line_number - 1] if line_number <= len(raw_lines) else ""
        if DOCTRINE_ALLOW in raw_line:
            continue

        for code, pattern, message in policies:
            if pattern.search(line):
                violations.append(
                    Violation(
                        path=normalized_path(path),
                        line=line_number,
                        code=code,
                        message=message,
                        excerpt=line.strip(),
                    )
                )

    return violations


def is_protocol_surface_path(path: Path) -> bool:
    normalized = normalized_path(path)
    return any(
        segment in normalized
        for segment in (
            "crates/andromeda-proto/",
            "crates/andromeda-quic/",
            "crates/andromeda-exec/",
        )
    )


def parse_enum_blocks(path: Path) -> list[EnumBlock]:
    active = strip_comments_preserve_lines(path.read_text(encoding="utf-8", errors="replace"))
    blocks: list[EnumBlock] = []

    for match in re.finditer(r"\benum\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{", active):
        enum_name = match.group(1)
        opening_brace = active.find("{", match.start())
        closing_brace = find_matching_brace(active, opening_brace)
        if closing_brace is None:
            blocks.append(
                EnumBlock(
                    path=path,
                    name=enum_name,
                    line=line_number_at(active, match.start()),
                    values={},
                    reserved=(),
                )
            )
            continue

        body = active[opening_brace + 1 : closing_brace]
        values = {
            value_match.group(1): int(value_match.group(2))
            for value_match in re.finditer(
                r"^\s*([A-Z][A-Z0-9_]*)\s*=\s*(-?\d+)\s*(?:\[[^\]]*\])?\s*;",
                body,
                re.MULTILINE,
            )
        }
        reserved = tuple(
            normalize_reserved(reserved_match.group(0))
            for reserved_match in re.finditer(r"^\s*reserved\s+[^;]+;", body, re.MULTILINE)
        )
        blocks.append(
            EnumBlock(
                path=path,
                name=enum_name,
                line=line_number_at(active, match.start()),
                values=values,
                reserved=reserved,
            )
        )

    return blocks


def find_matching_brace(text: str, opening_brace: int) -> int | None:
    depth = 0
    for index in range(opening_brace, len(text)):
        if text[index] == "{":
            depth += 1
        elif text[index] == "}":
            depth -= 1
            if depth == 0:
                return index
    return None


def line_number_at(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def normalize_reserved(line: str) -> str:
    return re.sub(r"\s+", " ", line.strip())


def scan_enum_drift(proto_files: Sequence[Path]) -> list[Violation]:
    blocks = [block for path in proto_files for block in parse_enum_blocks(path)]
    violations: list[Violation] = []

    for expected in EXPECTED_ENUMS:
        candidates = [
            block
            for block in blocks
            if normalized_path(block.path).endswith(expected.path_suffix)
            and block.name == expected.enum_name
        ]
        if not candidates:
            if any(normalized_path(path).endswith(expected.path_suffix) for path in proto_files):
                violations.append(
                    Violation(
                        path=expected.path_suffix,
                        line=1,
                        code="enum_drift",
                        message=f"Missing governed enum {expected.enum_name}.",
                    )
                )
            continue

        enum = candidates[0]
        violations.extend(compare_enum_values(enum, expected))
        violations.extend(compare_enum_reserved_ranges(enum, expected))

    return violations


def compare_enum_values(enum: EnumBlock, expected: ExpectedEnum) -> list[Violation]:
    violations: list[Violation] = []
    for name, expected_value in expected.values.items():
        actual = enum.values.get(name)
        if actual != expected_value:
            found = "missing" if actual is None else f"found {actual}"
            violations.append(
                Violation(
                    path=normalized_path(enum.path),
                    line=enum.line,
                    code="enum_drift",
                    message=(
                        f"{expected.enum_name}.{name} must remain {expected_value} "
                        f"({found})."
                    ),
                )
            )

    for name, actual in enum.values.items():
        if name not in expected.values:
            violations.append(
                Violation(
                    path=normalized_path(enum.path),
                    line=enum.line,
                    code="enum_drift",
                    message=(
                        f"{expected.enum_name}.{name} = {actual} is not in the governed "
                        "frame/status registry."
                    ),
                )
            )

    return violations


def compare_enum_reserved_ranges(enum: EnumBlock, expected: ExpectedEnum) -> list[Violation]:
    violations: list[Violation] = []
    declared = set(enum.reserved)
    for reserved in expected.reserved:
        if reserved not in declared:
            violations.append(
                Violation(
                    path=normalized_path(enum.path),
                    line=enum.line,
                    code="enum_drift",
                    message=f"{expected.enum_name} must preserve `{reserved}`.",
                )
            )
    return violations


def run_scan(
    roots: Sequence[Path] = DEFAULT_PROTO_ROOTS,
    policy_roots: Sequence[Path] = DEFAULT_POLICY_ROOTS,
) -> dict[str, object]:
    proto_files = discover_proto_files(roots)
    policy_files = discover_policy_files(policy_roots)
    violations: list[Violation] = []

    if not proto_files:
        violations.append(
            Violation(
                path=".",
                line=1,
                code="proto_missing",
                message="No .proto files found under governed protocol roots.",
            )
        )
    else:
        for path in proto_files:
            violations.extend(scan_proto_file(path))
        violations.extend(scan_enum_drift(proto_files))

    for path in policy_files:
        violations.extend(scan_policy_file(path))

    return {
        "schema": "andromeda.protocol_doctrine_scan.v1",
        "status": "FAIL" if violations else "PASS",
        "proto_count": len(proto_files),
        "policy_file_count": len(policy_files),
        "roots": [root.as_posix() for root in roots],
        "policy_roots": [root.as_posix() for root in policy_roots],
        "violations": [asdict(violation) for violation in violations],
    }


def normalized_path(path: Path) -> str:
    return path.as_posix()


def emit_github_annotations(report: dict[str, object]) -> None:
    for violation in report["violations"]:
        assert isinstance(violation, dict)
        print(
            "::error "
            f"file={violation['path']},line={violation['line']}"
            f"::{violation['message']}"
        )


def write_report(report: dict[str, object], output: Path | None) -> None:
    if output is None:
        return
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2, sort_keys=True), encoding="utf-8")


def parse_args(argv: Sequence[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        action="append",
        type=Path,
        dest="roots",
        help="Governed proto root or .proto file. May be passed more than once.",
    )
    parser.add_argument(
        "--policy-root",
        action="append",
        type=Path,
        dest="policy_roots",
        help="Governed dependency/source root. May be passed more than once.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Optional JSON report path for CI artifact upload.",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    roots = tuple(args.roots) if args.roots else DEFAULT_PROTO_ROOTS
    policy_roots = (
        tuple(args.policy_roots) if args.policy_roots else DEFAULT_POLICY_ROOTS
    )
    report = run_scan(roots, policy_roots)
    write_report(report, args.output)
    emit_github_annotations(report)
    print(json.dumps(report, indent=2, sort_keys=True))
    return 1 if report["status"] == "FAIL" else 0


if __name__ == "__main__":
    sys.exit(main())
