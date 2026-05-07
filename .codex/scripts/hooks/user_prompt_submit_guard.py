#!/usr/bin/env python3
import json
import re
import sys

payload = json.load(sys.stdin)
prompt = payload.get("prompt", "") or ""

blocked = [
    (r"\bdisable\s+(?:the\s+)?(wal|audit|security|recovery)\b", "Requests to disable WAL, audit, security, or recovery violate Andromeda C5 constraints."),
    (r"\bbypass\s+(?:the\s+)?(contract|permission|iam|audit|wal)\b", "Bypassing contracts, permissions, IAM, audit, or WAL is not allowed."),
    (r"\b(application|client).*\b(ad\s*hoc|freeform)\s+sql\b", "Andromeda rejects ad hoc SQL on the application surface."),
]

for pattern, reason in blocked:
    if re.search(pattern, prompt, flags=re.IGNORECASE | re.DOTALL):
        print(json.dumps({
            "decision": "block",
            "reason": reason,
            "hookSpecificOutput": {
                "hookEventName": "UserPromptSubmit",
                "additionalContext": "Reframe the request as a safe design, review, or migration task that preserves Andromeda invariants."
            }
        }))
        raise SystemExit(0)

print(json.dumps({
    "hookSpecificOutput": {
        "hookEventName": "UserPromptSubmit",
        "additionalContext": "Use the narrowest relevant Andromeda skill or specialist agent for this prompt."
    }
}))
