#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path
from common import emit, read_payload

payload = read_payload()
log_dir = Path(".codex/runtime")
log_dir.mkdir(parents=True, exist_ok=True)
with (log_dir / "hook_payloads.jsonl").open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(payload, ensure_ascii=False) + "\n")
emit({"systemMessage": "Hook payload logged locally under .codex/runtime/."})
