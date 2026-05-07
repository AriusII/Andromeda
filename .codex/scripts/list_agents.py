#!/usr/bin/env python3
from __future__ import annotations
import tomllib
from pathlib import Path

root = Path.cwd()
for path in sorted((root / ".codex" / "agents").glob("*.toml")):
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    print(f"{data.get('name')} [{data.get('sandbox_mode')}] - {data.get('description')}")
