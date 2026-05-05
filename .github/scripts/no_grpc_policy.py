#!/usr/bin/env python3
from pathlib import Path
import re, sys
files=[]
for root in [Path('schemas/proto'),Path('proto')]:
    if root.exists(): files += list(root.rglob('*.proto'))
patterns=[(re.compile(r'\bservice\s+\w+',re.I),'Protobuf service definitions are forbidden because Andromeda does not use gRPC.'),(re.compile(r'google\.api',re.I),'google.api annotations are not allowed.'),(re.compile(r'\bgrpc\b',re.I),'gRPC references are forbidden.')]
failed=False
for p in files:
    t=p.read_text(encoding='utf-8', errors='ignore')
    for rx,msg in patterns:
        if rx.search(t): print(f'::error file={p}::{msg}'); failed=True
if failed: sys.exit(1)
print(f'Protobuf governance passed for {len(files)} files.')
