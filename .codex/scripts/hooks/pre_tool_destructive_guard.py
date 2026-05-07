#!/usr/bin/env python3
from common import DESTRUCTIVE_PATTERNS, SECRET_PATTERNS, emit, event_context, event_deny_pretool, matches_any, read_payload, tool_command, tool_name

payload = read_payload()
name = tool_name(payload)
command = tool_command(payload)

if name.lower() in {"bash", "shell"} or command:
    pattern = matches_any(DESTRUCTIVE_PATTERNS, command)
    if pattern:
        emit(event_deny_pretool(f"Blocked destructive command by Andromeda Codex hook. Pattern: {pattern}. Use a bounded cleanup plan and request explicit approval."))
        raise SystemExit(0)

    secret = matches_any(SECRET_PATTERNS, command)
    if secret:
        emit(event_deny_pretool(f"Blocked command that appears to expose or manipulate secret material. Pattern: {secret}."))
        raise SystemExit(0)

emit(event_context("PreToolUse", "PreToolUse guard passed. For Rust or Codex tooling changes, run targeted validation before final response."))
