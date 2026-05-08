#!/usr/bin/env python3
"""Read-only unsafe Rust inventory for Andromeda."""

from __future__ import annotations

import argparse
import bisect
import re
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]

EXCLUDED_DIRS = {
    ".agents",
    ".codex",
    ".git",
    ".idea",
    ".venv",
    "node_modules",
    "target",
}

UNSAFE_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    (
        "unsafe_function",
        re.compile(r"(?<!r#)\bunsafe\b\s+(?:(?:async|const|extern)\b\s+)*fn\b"),
    ),
    ("unsafe_trait", re.compile(r"(?<!r#)\bunsafe\b\s+trait\b")),
    ("unsafe_impl", re.compile(r"(?<!r#)\bunsafe\b\s+impl\b")),
    ("unsafe_block", re.compile(r"(?<!r#)\bunsafe\b\s*\{")),
    ("unsafe_block", re.compile(r"(?<!r#)\bunsafe\b\s+extern\b\s*\{")),
)

LOCK_FREE_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    (
        "lock_free_atomic_type",
        re.compile(
            r"\bAtomic(?:Bool|I8|I16|I32|I64|Isize|Ptr|U8|U16|U32|U64|Usize)\b",
        ),
    ),
    (
        "lock_free_atomic_ordering",
        re.compile(r"\bOrdering::(?:Relaxed|Acquire|Release|AcqRel|SeqCst)\b"),
    ),
    ("lock_free_compare_exchange", re.compile(r"\bcompare_exchange(?:_weak)?\b")),
    (
        "lock_free_fetch_update",
        re.compile(r"\bfetch_(?:add|sub|and|or|xor|nand|max|min|update)\b"),
    ),
)


@dataclass(frozen=True, order=True)
class Finding:
    path: str
    line: int
    classification: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Read-only inventory of unsafe Rust surfaces. Output lines use "
            "path:line:classification."
        ),
    )
    parser.add_argument(
        "paths",
        nargs="*",
        help="Optional files or directories to scan. Defaults to the repository root.",
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=ROOT,
        help="Repository root used for relative output paths.",
    )
    parser.add_argument(
        "--include-lock-free",
        action="store_true",
        help="Also report advisory atomic and lock-free indicators.",
    )
    parser.add_argument(
        "--summary",
        action="store_true",
        help="Print a classification count after inventory lines.",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit with status 1 when inventory entries are found.",
    )
    return parser.parse_args()


def preserve_newlines_as_spaces(text: str) -> str:
    return "".join("\n" if char == "\n" else " " for char in text)


def raw_string_end(text: str, start: int) -> int | None:
    for prefix in ("br", "r"):
        if not text.startswith(prefix, start):
            continue
        cursor = start + len(prefix)
        while cursor < len(text) and text[cursor] == "#":
            cursor += 1
        if cursor >= len(text) or text[cursor] != '"':
            continue
        hashes = cursor - start - len(prefix)
        marker = '"' + ("#" * hashes)
        end = text.find(marker, cursor + 1)
        if end == -1:
            return len(text)
        return end + len(marker)
    return None


def quoted_end(text: str, start: int, quote: str) -> int:
    cursor = start + 1
    while cursor < len(text):
        char = text[cursor]
        if char == "\\":
            cursor += 2
            continue
        cursor += 1
        if char == quote:
            return cursor
    return len(text)


def block_comment_end(text: str, start: int) -> int:
    depth = 1
    cursor = start + 2
    while cursor < len(text) and depth > 0:
        pair = text[cursor : cursor + 2]
        if pair == "/*":
            depth += 1
            cursor += 2
            continue
        if pair == "*/":
            depth -= 1
            cursor += 2
            continue
        cursor += 1
    return cursor


def strip_comments_and_strings(text: str) -> str:
    pieces: list[str] = []
    cursor = 0
    while cursor < len(text):
        raw_end = raw_string_end(text, cursor)
        if raw_end is not None:
            pieces.append(preserve_newlines_as_spaces(text[cursor:raw_end]))
            cursor = raw_end
            continue

        pair = text[cursor : cursor + 2]
        if pair == "//":
            end = text.find("\n", cursor)
            if end == -1:
                end = len(text)
            pieces.append(preserve_newlines_as_spaces(text[cursor:end]))
            cursor = end
            continue
        if pair == "/*":
            end = block_comment_end(text, cursor)
            pieces.append(preserve_newlines_as_spaces(text[cursor:end]))
            cursor = end
            continue

        char = text[cursor]
        if char == '"':
            end = quoted_end(text, cursor, '"')
            pieces.append(preserve_newlines_as_spaces(text[cursor:end]))
            cursor = end
            continue
        if pair in {'b"', 'c"'}:
            end = quoted_end(text, cursor + 1, '"')
            pieces.append(preserve_newlines_as_spaces(text[cursor:end]))
            cursor = end
            continue
        if char == "'" and cursor + 1 < len(text) and text[cursor + 1] != "_":
            end = quoted_end(text, cursor, "'")
            if end - cursor <= 8:
                pieces.append(preserve_newlines_as_spaces(text[cursor:end]))
                cursor = end
                continue

        pieces.append(char)
        cursor += 1
    return "".join(pieces)


def line_starts(text: str) -> list[int]:
    starts = [0]
    starts.extend(index + 1 for index, char in enumerate(text) if char == "\n")
    return starts


def line_for(starts: list[int], offset: int) -> int:
    return bisect.bisect_right(starts, offset)


def rel(root: Path, path: Path) -> str:
    try:
        return path.relative_to(root).as_posix()
    except ValueError:
        return path.as_posix()


def is_excluded(path: Path) -> bool:
    return any(part in EXCLUDED_DIRS for part in path.parts)


def rust_files(root: Path, requested_paths: list[str]) -> list[Path]:
    roots = [Path(raw) for raw in requested_paths] if requested_paths else [root]
    files: list[Path] = []
    for raw_path in roots:
        path = raw_path if raw_path.is_absolute() else root / raw_path
        if path.is_file() and path.suffix == ".rs":
            files.append(path)
            continue
        if path.is_dir():
            files.extend(
                candidate
                for candidate in path.rglob("*.rs")
                if candidate.is_file() and not is_excluded(candidate)
            )
    return sorted(set(files))


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        return ""


def scan_file(
    root: Path,
    path: Path,
    patterns: tuple[tuple[str, re.Pattern[str]], ...],
) -> list[Finding]:
    text = read_text(path)
    stripped = strip_comments_and_strings(text)
    starts = line_starts(stripped)
    findings: set[Finding] = set()
    relative_path = rel(root, path)
    for classification, pattern in patterns:
        for match in pattern.finditer(stripped):
            findings.add(
                Finding(
                    path=relative_path,
                    line=line_for(starts, match.start()),
                    classification=classification,
                ),
            )
    return sorted(findings)


def print_summary(findings: list[Finding]) -> None:
    counts: dict[str, int] = {}
    for finding in findings:
        counts[finding.classification] = counts.get(finding.classification, 0) + 1

    print()
    print("Summary")
    if not counts:
        print("  none")
        return
    for classification in sorted(counts):
        print(f"  {classification}: {counts[classification]}")


def main() -> int:
    args = parse_args()
    root = args.root.resolve()
    patterns = UNSAFE_PATTERNS
    if args.include_lock_free:
        patterns = (*patterns, *LOCK_FREE_PATTERNS)

    findings: list[Finding] = []
    for path in rust_files(root, args.paths):
        findings.extend(scan_file(root, path, patterns))

    findings = sorted(set(findings))
    for finding in findings:
        print(f"{finding.path}:{finding.line}:{finding.classification}")

    if args.summary:
        print_summary(findings)

    return 1 if args.strict and findings else 0


if __name__ == "__main__":
    raise SystemExit(main())
