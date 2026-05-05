#!/usr/bin/env python3
from pathlib import Path
import re, sys
ROOT=Path.cwd(); TEXT_EXTS={'.rs','.md','.toml','.yaml','.yml','.proto','.txt'}
EXCLUDE={'.git','target','dist'}
FORBIDDEN=[(re.compile(r'\bgrpc\b',re.I),'gRPC is forbidden. Use QUIC + custom RPC + Protobuf contracts.'),(re.compile(r'\btonic\b',re.I),'tonic implies gRPC and is forbidden in protocol paths.'),(re.compile(r'\bSELECT\s+\*',re.I),'SELECT * is forbidden in Andromeda examples and design.')]
ALLOW=['.github/docs','.github/scripts','.github/workflows','README','AGENTS']
def excluded(p): return bool(set(p.parts)&EXCLUDE) or p.suffix not in TEXT_EXTS
def allowed(p): return any(a in str(p).replace('\\','/') for a in ALLOW)
fail=[]
for p in ROOT.rglob('*'):
    if p.is_file() and not excluded(p):
        t=p.read_text(encoding='utf-8', errors='ignore')
        for rx,msg in FORBIDDEN:
            if rx.search(t) and not allowed(p): fail.append((p,msg))
if fail:
    for p,msg in fail: print(f'::error file={p}::{msg}')
    sys.exit(1)
print('Andromeda policy gate passed.')
