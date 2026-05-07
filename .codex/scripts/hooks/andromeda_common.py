#!/usr/bin/env python3
"""Common helpers for Andromeda Claude Code hooks."""
from __future__ import annotations

import json
import os
import re
import sys
from typing import Any

FORBIDDEN_DESIGN_PATTERNS = [
    (re.compile(r"\bgrpc\b", re.IGNORECASE), "Do not introduce gRPC. Andromeda uses QUIC with Protobuf contracts."),
    (re.compile(r"\bad\s*hoc\s+sql\b", re.IGNORECASE), "Do not introduce ad hoc SQL as a native application surface."),
    (re.compile(r"\bSELECT\s+\*\b", re.IGNORECASE), "Do not normalize SELECT * examples in SRPL or native APIs."),
]

DANGEROUS_SHELL_PATTERNS = [
    (re.compile(r"\brm\s+-rf\s+(/|\$HOME|~|\.)"), "Destructive recursive removal requires explicit human approval."),
    (re.compile(r"\bchmod\s+-R\s+777\b"), "World-writable recursive permissions are not allowed."),
    (re.compile(r"\b(?:cat|type)\s+.*(?:id_rsa|\.pem|\.key|token|secret|password)", re.IGNORECASE), "Do not print secrets or private keys."),
    (re.compile(r"\b(?:curl|wget)\b.*\|\s*(?:sh|bash|zsh|python)"), "Do not pipe remote code directly into an interpreter."),
]

def load_event() -> dict[str, Any]:
    raw = sys.stdin.read()
    if not raw.strip():
        return {}
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return {"raw": raw}

def text_from_event(event: dict[str, Any]) -> str:
    values: list[str] = []
    def walk(x: Any) -> None:
        if isinstance(x, str):
            values.append(x)
        elif isinstance(x, dict):
            for v in x.values():
                walk(v)
        elif isinstance(x, list):
            for v in x:
                walk(v)
    walk(event)
    return "\n".join(values)

def deny(reason: str) -> None:
    print(json.dumps({
        "continue": False,
        "stopReason": reason,
        "suppressOutput": False,
        "systemMessage": reason
    }))
    sys.exit(0)

def warn(message: str) -> None:
    print(json.dumps({
        "continue": True,
        "suppressOutput": False,
        "systemMessage": message
    }))
    sys.exit(0)

def allow(message: str | None = None) -> None:
    payload = {"continue": True, "suppressOutput": True}
    if message:
        payload["systemMessage"] = message
        payload["suppressOutput"] = False
    print(json.dumps(payload))
    sys.exit(0)
