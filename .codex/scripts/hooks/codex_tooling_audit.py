#!/usr/bin/env python3
from common import emit, event_context, read_payload, tool_command, tool_name

payload = read_payload()
name = tool_name(payload).lower()
text = f"{tool_command(payload)}\n{payload}"
mutative = name in {"write", "edit", "multiedit", "apply_patch"} or any(
    token in text.lower()
    for token in [
        "apply_patch",
        "git add",
        "git commit",
        "set-content",
        "add-content",
        "out-file",
        "remove-item",
        "move-item",
        "copy-item",
    ]
)

if mutative and (".codex" in text or ".agents" in text):
    message = "Codex tooling audit reminder: agents, skills, hooks, and prompts must remain distinct. Validate registry and frontmatter before packaging."
else:
    message = "Codex tooling audit complete."

emit(event_context("PostToolUse", message))
