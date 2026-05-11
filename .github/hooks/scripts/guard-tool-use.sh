#!/usr/bin/env bash
set -euo pipefail

input="$(cat | tr -d '\r')"

run_python() {
  if command -v python3 >/dev/null 2>&1; then
    python3 "$@"
  elif command -v python >/dev/null 2>&1; then
    python "$@"
  else
    exit 0
  fi
}

HOOK_INPUT="$input" run_python -c '
import json
import os
import re
import sys

raw = os.environ.get("HOOK_INPUT", "")
try:
    event = json.loads(raw or "{}")
except Exception:
    sys.exit(0)

tool_name = str(event.get("toolName") or event.get("tool_name") or "")
args_raw = event.get("toolArgs") if "toolArgs" in event else event.get("tool_args", {})
if isinstance(args_raw, str):
    try:
        tool_args = json.loads(args_raw or "{}")
    except Exception:
        tool_args = {}
elif isinstance(args_raw, dict):
    tool_args = args_raw
else:
    tool_args = {}

short_name = tool_name.split(".")[-1]

def deny(reason):
    print(json.dumps({"permissionDecision": "deny", "permissionDecisionReason": reason}, separators=(",", ":")))
    sys.exit(0)

def normalize_path(value):
    path = str(value or "").replace("\\\\", "/").replace("\\", "/")
    while path.startswith("./"):
        path = path[2:]
    return path

def path_reason(path):
    if re.match(r"^docs/", path):
        return "docs/ is read-only by repository policy"
    if re.match(r"^\.codex/", path):
        return ".codex/ is read-only by repository policy"
    if re.match(r"^\.github/copilot-instructions\.md$", path):
        return ".github/copilot-instructions.md is managed and must not be edited"
    if re.match(r"^Cargo\.lock$", path):
        if not (event.get("explicitLockUpdate") or event.get("lockUpdateTask") or tool_args.get("explicitLockUpdate") or tool_args.get("lockUpdateTask")):
            return "Cargo.lock changes require an explicit lock-update task"
    return None

def iter_paths(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for item in value:
            yield from iter_paths(item)
    elif isinstance(value, dict):
        for key in ("path", "file_path", "pathInProject", "filePath", "notebook_path"):
            if key in value:
                yield from iter_paths(value[key])
        for key in ("edits", "operations", "files"):
            if key in value:
                yield from iter_paths(value[key])

write_tools = {"edit", "create", "Write", "MultiEdit", "NotebookEdit"}
if short_name in write_tools or tool_name in write_tools:
    for raw_path in iter_paths(tool_args):
        path = normalize_path(raw_path)
        reason = path_reason(path)
        if reason:
            deny(reason)

shell_tools = {"bash", "shell", "execute", "powershell"}
if short_name in shell_tools or tool_name in shell_tools:
    command = str(tool_args.get("command") or tool_args.get("cmd") or tool_args.get("script") or "")
    checks = [
        (r"rm\s+-rf\s+/", "Refusing dangerous recursive removal of filesystem root"),
        (r"Remove-Item\s+-Recurse\s+-Force\s+C:\\+", "Refusing dangerous recursive removal of C:\\"),
        (r"\bformat\b", "Refusing dangerous format command"),
        (r"DROP\s+TABLE", "Refusing destructive DROP TABLE command"),
        (r"git\s+push\b(?=.*--force)(?=.*\b(main|master|trunk|production|prod|release)\b)", "Refusing force push to a protected branch"),
    ]
    for pattern, reason in checks:
        if re.search(pattern, command, re.IGNORECASE | re.DOTALL):
            deny(reason)

sys.exit(0)
'