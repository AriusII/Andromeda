#!/usr/bin/env python3
import hashlib
import json
from pathlib import Path

PROTO_ROOTS = (
    Path(".codex/schemas/proto"),
    Path("proto"),
    Path("crates/andromeda-proto/proto"),
)


def build_manifest(repository_root=Path(".")):
    repository_root = Path(repository_root)
    items = []
    for root in PROTO_ROOTS:
        proto_root = repository_root / root
        if proto_root.exists():
            for path in sorted(proto_root.rglob("*.proto")):
                data = path.read_bytes()
                relative_path = path.relative_to(repository_root)
                items.append(
                    {
                        "path": relative_path.as_posix(),
                        "sha256": hashlib.sha256(data).hexdigest(),
                        "bytes": len(data),
                    }
                )

    items.sort(key=lambda item: item["path"])
    return {
        "schema": "andromeda.protobuf_contract_manifest.v1",
        "contract_count": len(items),
        "contracts": items,
    }


def main():
    manifest = build_manifest()
    Path("target").mkdir(exist_ok=True)
    manifest_json = json.dumps(manifest, indent=2)
    Path("target/protobuf-contract-manifest.json").write_text(
        manifest_json,
        encoding="utf-8",
    )
    print(manifest_json)


if __name__ == "__main__":
    main()
