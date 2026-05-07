#!/usr/bin/env python3
from common import DESTRUCTIVE_PATTERNS, emit, event_permission_decision, matches_any, read_payload, tool_command

payload = read_payload()
command = tool_command(payload)

pattern = matches_any(DESTRUCTIVE_PATTERNS, command)
if pattern:
    emit(event_permission_decision("deny", f"Permission denied by Andromeda policy for destructive command pattern: {pattern}."))
    raise SystemExit(0)

risky_network = any(token in command.lower() for token in ["curl ", "wget ", "Invoke-WebRequest".lower(), "ssh ", "scp ", "nc "])
if risky_network and "official" not in command.lower() and "github.com" not in command.lower():
    emit(event_permission_decision("deny", "Network-like command denied unless it targets an explicit official source and the task requires it."))
    raise SystemExit(0)

emit(event_permission_decision("allow"))
