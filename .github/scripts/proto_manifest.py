#!/usr/bin/env python3
from pathlib import Path
import hashlib, json
items=[]
for root in [Path('.codex/schemas/proto'), Path('proto')]:
    if root.exists():
        for p in sorted(root.rglob('*.proto')):
            data=p.read_bytes(); items.append({'path':str(p).replace('\\','/'),'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data)})
Path('target').mkdir(exist_ok=True)
manifest={'schema':'andromeda.protobuf_contract_manifest.v1','contract_count':len(items),'contracts':items}
Path('target/protobuf-contract-manifest.json').write_text(json.dumps(manifest,indent=2),encoding='utf-8')
print(json.dumps(manifest,indent=2))
