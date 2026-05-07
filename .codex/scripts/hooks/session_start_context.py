#!/usr/bin/env python3
import json

context = """
Andromeda reminder: preserve strict procedure-only RPC surface, typed SRPL contracts, WAL-before-visible-commit, versioned catalog/statistics/policies, explicit security/audit, and crash-recoverable storage. Do not introduce ad hoc SQL application APIs, GPU commit-path logic, unbounded SRPL loops, implicit NULL semantics, or unobservable optimizer decisions.
""".strip()

print(json.dumps({
    "hookSpecificOutput": {
        "hookEventName": "SessionStart",
        "additionalContext": context
    }
}))
