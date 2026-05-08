from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path
from typing import Any

REPO_ROOT = Path.cwd()

CORE_CONTEXT = """Andromeda context:
- Application access is RPC-only through typed, cataloged Procedures.
- SRPL is strict, relational, typed, set-oriented, and bounded.
- Commit visible requires durable WAL.
- Truth is the latest valid cold snapshot plus durable WAL from that snapshot.
- GPU and learned components are outside commit, WAL, rollback, recovery, catalog publication, and security-critical paths.
- Rust persistent and network formats must use explicit codecs, not native struct layout.
- Critical decisions must be versioned, bounded, observable, explainable, and disableable.
"""

def read_payload() -> dict[str, Any]:
    try:
        raw = sys.stdin.read()
        if not raw.strip():
            return {}
        value = json.loads(raw)
        return value if isinstance(value, dict) else {"value": value}
    except Exception as exc:
        return {"_hook_parse_error": str(exc)}

def hook_parse_error(payload: dict[str, Any]) -> str | None:
    value = payload.get("_hook_parse_error")
    return value if isinstance(value, str) and value else None

def emit(obj: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(obj, ensure_ascii=False) + "\n")
    sys.stdout.flush()

def event_context(event_name: str, text: str) -> dict[str, Any]:
    return {
        "hookSpecificOutput": {
            "hookEventName": event_name,
            "additionalContext": text,
        }
    }

def event_deny_pretool(reason: str) -> dict[str, Any]:
    return {
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }
    }

def event_permission_decision(behavior: str, message: str | None = None) -> dict[str, Any]:
    decision: dict[str, Any] = {"behavior": behavior}
    if message:
        decision["message"] = message
    return {
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": decision,
        }
    }

def tool_command(payload: dict[str, Any]) -> str:
    tool_input = payload.get("tool_input")
    if isinstance(tool_input, dict):
        command = tool_input.get("command")
        if isinstance(command, str):
            return command
    command = payload.get("command")
    return command if isinstance(command, str) else ""

def tool_name(payload: dict[str, Any]) -> str:
    value = payload.get("tool_name") or payload.get("tool")
    return value if isinstance(value, str) else ""

def prompt_text(payload: dict[str, Any]) -> str:
    value = payload.get("prompt") or payload.get("user_prompt") or payload.get("input")
    return value if isinstance(value, str) else ""

DESTRUCTIVE_PATTERNS = [
    r"\brm\s+-rf\s+[/~.$]?",
    r"\bgit\s+reset\s+--hard\b",
    r"\bgit\s+clean\s+-[a-zA-Z]*[fdx]",
    r"\bdd\s+if=",
    r"\bmkfs\.",
    r"\bchmod\s+-R\s+777\b",
    r"\bchown\s+-R\b",
    r">\s*/dev/sd[a-z]",
]

SECRET_PATTERNS = [
    r"OPENAI_API_KEY",
    r"GITHUB_TOKEN",
    r"AZURE_CLIENT_SECRET",
    r"AWS_SECRET_ACCESS_KEY",
    r"PRIVATE_KEY",
    r"BEGIN RSA PRIVATE KEY",
    r"BEGIN OPENSSH PRIVATE KEY",
]

def matches_any(patterns: list[str], text: str) -> str | None:
    for pattern in patterns:
        if re.search(pattern, text, flags=re.IGNORECASE):
            return pattern
    return None
