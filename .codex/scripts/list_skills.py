#!/usr/bin/env python3
from __future__ import annotations
import re
from pathlib import Path
for p in sorted((Path.cwd()/'.codex'/'skills').glob('*/SKILL.md')):
    text = p.read_text(encoding='utf-8')
    fm = text.split('---',2)[1]
    name = re.search(r'^name:\s*(.+)$', fm, re.M).group(1).strip()
    desc = re.search(r'^description:\s*"?(.+?)"?$', fm, re.M).group(1).strip()
    print(f"{name}\t{desc}")
