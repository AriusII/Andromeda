#!/usr/bin/env python3
import json
import re
import sys

payload = json.load(sys.stdin)
tool_input = payload.get("tool_input") or {}
command = tool_input.get("command", "") if isinstance(tool_input, dict) else ""

rules = [
    (r"rm\s+-rf\s+(/|~|\$HOME)(\s|$)", "Refuse destructive recursive deletion outside an explicit sandbox path."),
    (r"git\s+push(\s|$)", "Refuse git push from an automated Codex hook-governed session unless explicitly approved by the user."),
    (r"curl\s+.*\|\s*(sh|bash|zsh)", "Refuse pipe-to-shell installation pattern."),
    (r"chmod\s+-R\s+777\b", "Refuse broad world-writable permission changes."),
    (r"(?i)(prod|production).*(drop|truncate|delete|restore)", "Refuse production-destructive operation patterns."),
    (r"(?i)(OPENAI_API_KEY|SECRET|PASSWORD|TOKEN)=", "Refuse commands that appear to inline secrets."),
]

for pattern, reason in rules:
    if re.search(pattern, command):
        print(json.dumps({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": reason,
                "additionalContext": "Use a safer, reviewable command scoped to the repository workspace."
            }
        }))
        raise SystemExit(0)

print(json.dumps({"hookSpecificOutput": {"hookEventName": "PreToolUse"}}))
