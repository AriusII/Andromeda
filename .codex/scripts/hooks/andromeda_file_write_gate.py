#!/usr/bin/env python3
from __future__ import annotations

import re
from pathlib import Path
from typing import Any

from common import emit, event_context, event_deny_pretool, hook_parse_error, read_payload

REPO_ROOT = Path.cwd().resolve()

PATH_KEYS = {"file_path", "filepath", "path", "target_file", "target_path"}
CONTENT_KEYS = {"content", "new_string", "replacement", "text", "body"}

SENSITIVE_PATH_PARTS = {
    ".git",
    ".ssh",
    ".gnupg",
    ".aws",
    ".azure",
    ".kube",
    "node_modules",
    "target",
}

SENSITIVE_FILE_PATTERNS = [
    re.compile(r"(^|[\\/])\.env(?:\.|$)", re.IGNORECASE),
    re.compile(r"(^|[\\/])id_(?:rsa|dsa|ecdsa|ed25519)$", re.IGNORECASE),
    re.compile(r"\.(?:pem|key|p12|pfx|crt|cer)$", re.IGNORECASE),
    re.compile(r"(?:secret|credential|token|password)s?\.(?:json|ya?ml|toml|txt|env)$", re.IGNORECASE),
]

SECRET_CONTENT_PATTERNS = [
    re.compile(r"-----BEGIN (?:RSA |DSA |EC |OPENSSH |PGP )?PRIVATE KEY-----"),
    re.compile(r"\bAKIA[0-9A-Z]{16}\b"),
    re.compile(r"\b(?:ghp|gho|ghu|ghs|github_pat)_[A-Za-z0-9_]{30,}\b"),
    re.compile(r"\bsk-[A-Za-z0-9_-]{32,}\b"),
    re.compile(r"\b(?:api[_-]?key|secret|token|password|passwd)\b\s*[:=]\s*[\"']?[A-Za-z0-9_./+=:@%-]{16,}", re.IGNORECASE),
]


def walk_values(value: Any, key: str | None = None) -> tuple[list[str], list[str]]:
    paths: list[str] = []
    contents: list[str] = []

    if isinstance(value, dict):
        for child_key, child_value in value.items():
            child_paths, child_contents = walk_values(child_value, str(child_key))
            paths.extend(child_paths)
            contents.extend(child_contents)
    elif isinstance(value, list):
        for child in value:
            child_paths, child_contents = walk_values(child, key)
            paths.extend(child_paths)
            contents.extend(child_contents)
    elif isinstance(value, str) and key:
        normalized_key = key.lower()
        if normalized_key in PATH_KEYS:
            paths.append(value)
        elif normalized_key in CONTENT_KEYS:
            contents.append(value)

    return paths, contents


def resolve_repo_path(raw_path: str) -> Path | None:
    candidate = Path(raw_path)
    if not candidate.is_absolute():
        candidate = REPO_ROOT / candidate
    try:
        return candidate.resolve(strict=False)
    except OSError:
        return None


def is_relative_to(path: Path, parent: Path) -> bool:
    try:
        path.relative_to(parent)
        return True
    except ValueError:
        return False


def sensitive_path_reason(raw_path: str) -> str | None:
    resolved = resolve_repo_path(raw_path)
    if resolved is None:
        return f"unresolvable path: {raw_path}"

    if not is_relative_to(resolved, REPO_ROOT):
        return f"path outside repository workspace: {raw_path}"

    relative = resolved.relative_to(REPO_ROOT)
    parts = {part.lower() for part in relative.parts}
    matched_part = parts & SENSITIVE_PATH_PARTS
    if matched_part:
        return f"sensitive repository path component: {sorted(matched_part)[0]}"

    relative_text = relative.as_posix()
    for pattern in SENSITIVE_FILE_PATTERNS:
        if pattern.search(relative_text):
            return f"sensitive file path pattern: {pattern.pattern}"

    return None


def secret_content_reason(text: str) -> str | None:
    for pattern in SECRET_CONTENT_PATTERNS:
        if pattern.search(text):
            return f"secret-like content pattern: {pattern.pattern}"
    return None


payload = read_payload()
parse_error = hook_parse_error(payload)
if parse_error:
    emit(event_deny_pretool(f"Blocked PreToolUse because hook input could not be parsed: {parse_error}."))
    raise SystemExit(0)

tool_input = payload.get("tool_input")
paths, contents = walk_values(tool_input if isinstance(tool_input, dict) else payload)

if not paths:
    emit(event_deny_pretool("Blocked file write because no target path was present in the hook payload."))
    raise SystemExit(0)

for raw_path in paths:
    reason = sensitive_path_reason(raw_path)
    if reason:
        emit(event_deny_pretool(f"Blocked file write by Andromeda file gate: {reason}."))
        raise SystemExit(0)

for content in contents:
    reason = secret_content_reason(content)
    if reason:
        emit(event_deny_pretool(f"Blocked file write by Andromeda file gate: {reason}."))
        raise SystemExit(0)

emit(event_context("PreToolUse", "File write gate passed. Target path is within the repository and no obvious secret material was detected."))
