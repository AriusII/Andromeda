#!/usr/bin/env python3
from pathlib import Path
import sys
failed=False
targets=list(Path('.github').rglob('*.yml'))+list(Path('.github').rglob('*.yaml'))
for p in targets:
    t=p.read_text(encoding='utf-8', errors='ignore')
    if '\t' in t:
        print(f'::error file={p}::Tabs are not allowed in YAML.'); failed=True
    if '\r\n' in t:
        print(f'::warning file={p}::Prefer LF line endings.')
if failed: sys.exit(1)
print(f'YAML sanity passed for {len(targets)} files.')
