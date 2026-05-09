#!/usr/bin/env python3
from __future__ import annotations
import tomllib
from pathlib import Path
root = Path.cwd()/'.codex'
config = tomllib.loads((root/'config.toml').read_text(encoding='utf-8'))
for name, entry in sorted(config.get('agents', {}).items()):
    print(f"{name}\t{entry.get('kind','')}\t{entry.get('description','')}")
