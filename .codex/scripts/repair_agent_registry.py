#!/usr/bin/env python3
from __future__ import annotations
import tomllib
from pathlib import Path
root = Path.cwd()/'.codex'
config_path = root/'config.toml'
config_text = config_path.read_text(encoding='utf-8')
head = config_text.split('\n[agents.',1)[0].rstrip() + '\n\n'
entries=[]
for p in sorted((root/'agents').glob('*.toml')):
    doc = tomllib.loads(p.read_text(encoding='utf-8'))
    name = doc['name']
    desc = doc.get('description','')
    kind = doc.get('agent_kind','')
    entries.append(f'[agents.{name}]\nconfig_file = "agents/{p.name}"\ndescription = "{desc}"\nkind = "{kind}"\n')
config_path.write_text(head+'\n'.join(entries), encoding='utf-8')
print(f'Registered {len(entries)} agents in .codex/config.toml')
