#!/usr/bin/env python3
from __future__ import annotations
import re
from pathlib import Path

root = Path.cwd()
for path in sorted((root / ".agents" / "skills").glob("*/SKILL.md")):
    text = path.read_text(encoding="utf-8")
    name = re.search(r"^name:\s*([-a-z0-9]+)", text, re.M)
    desc = re.search(r"^description:\s*(.+)", text, re.M)
    print(f"{name.group(1) if name else path.parent.name} - {desc.group(1) if desc else ''}")
