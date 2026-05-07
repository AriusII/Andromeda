#!/usr/bin/env python3
from common import emit, event_context, read_payload, tool_command

payload = read_payload()
command = tool_command(payload)

notes = []
if "apply_patch" in str(payload) or "cat >" in command or "python" in command:
    notes.append("If files changed, validate the modified surface before final response.")
if "cargo" in command:
    notes.append("Record cargo command outcome and relevant failures.")
if ".codex" in command or ".agents" in command:
    notes.append("Run python3 .codex/scripts/validate_codex_tooling.py after Codex tooling edits.")

if not notes:
    notes.append("PostToolUse audit complete.")

emit(event_context("PostToolUse", "\n".join(notes)))
