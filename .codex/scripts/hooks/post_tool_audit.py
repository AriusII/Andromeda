#!/usr/bin/env python3
from __future__ import annotations

import re

from common import emit, event_context, read_payload, tool_command, tool_name

payload = read_payload()
name = tool_name(payload)
command = tool_command(payload)

MUTATIVE_COMMAND_PATTERNS = [
    r"\bapply_patch\b",
    r"\bgit\s+(?:add|commit|mv|rm|restore|reset|clean|checkout|switch|merge|rebase|cherry-pick|stash|tag|push)\b",
    r"\bcargo\s+fmt\b",
    r"\b(?:rm|mv|cp|mkdir|touch|chmod|chown)\b",
    r"\b(?:Remove-Item|Move-Item|Copy-Item|New-Item|Set-Content|Add-Content|Out-File)\b",
    r">\s*[^&]",
    r">>",
    r"\bpython3?\b\s+\S*[\\/](?:generate|update|write|sync|format|fix|migrate|scaffold)[-_A-Za-z0-9]*\.py\b",
    r"\bpython3?\b\s+(?:generate|update|write|sync|format|fix|migrate|scaffold)[-_A-Za-z0-9]*\.py\b",
]


def is_write_tool(tool: str) -> bool:
    return tool.lower() in {"write", "edit", "multiedit", "apply_patch"}


def is_mutative_command(value: str) -> bool:
    return any(re.search(pattern, value, flags=re.IGNORECASE) for pattern in MUTATIVE_COMMAND_PATTERNS)


notes = []
mutative = is_write_tool(name) or is_mutative_command(command)

if mutative:
    notes.append("If files changed, validate the modified surface before final response.")
if "cargo" in command:
    notes.append("Record cargo command outcome and relevant failures.")
if mutative and (".codex" in command or ".agents" in command or ".codex" in str(payload) or ".agents" in str(payload)):
    notes.append("Run python3 .codex/scripts/validate_codex_tooling.py after Codex tooling edits.")

if not notes:
    notes.append("PostToolUse audit complete.")

emit(event_context("PostToolUse", "\n".join(notes)))
