#!/usr/bin/env python3
from pathlib import Path
import re, sys
paths=[]
for pattern in ['.github/copilot-instructions.md','.github/instructions/**/*.md','AGENTS.md']:
    paths += list(Path('.').glob(pattern))
failed=False
for p in paths:
    t=p.read_text(encoding='utf-8', errors='ignore')
    if 'Andromeda' not in t: print(f'::warning file={p}::Instruction file does not mention Andromeda.')
    if re.search(r'\bgrpc\b', t, re.I) and 'forbidden' not in t.lower() and 'do not' not in t.lower():
        print(f'::error file={p}::AI instructions must not recommend gRPC.'); failed=True
if failed: sys.exit(1)
print(f'AI instruction policy passed for {len(paths)} files.')
