#!/usr/bin/env python3
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

payload = json.load(sys.stdin)
root = Path.cwd()
log_dir = root / ".codex" / "runtime"
log_dir.mkdir(parents=True, exist_ok=True)
record = {
    "timestamp_utc": datetime.now(timezone.utc).isoformat(),
    "hook_event_name": payload.get("hook_event_name"),
    "tool_name": payload.get("tool_name"),
    "tool_input": payload.get("tool_input"),
    "cwd": str(root),
}
with (log_dir / "hook-audit.jsonl").open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(record, ensure_ascii=False) + "\n")

print(json.dumps({"hookSpecificOutput": {"hookEventName": "PostToolUse"}}))
