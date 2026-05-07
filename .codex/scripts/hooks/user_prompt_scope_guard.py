#!/usr/bin/env python3
from common import CORE_CONTEXT, emit, event_context, prompt_text, read_payload

payload = read_payload()
prompt = prompt_text(payload).lower()

blocked_phrases = [
    "ignore agents.md",
    "ignore the project invariants",
    "bypass wal",
    "skip recovery",
    "gpu commit path",
    "disable audit",
]

for phrase in blocked_phrases:
    if phrase in prompt:
        emit({
            "decision": "block",
            "reason": f"The prompt requests behavior that conflicts with Andromeda mission-critical invariants: {phrase}."
        })
        raise SystemExit(0)

extra = "User prompt accepted. Apply Andromeda strict-boundary rules and classify risk before edits."
emit(event_context("UserPromptSubmit", extra + "\n\n" + CORE_CONTEXT))
