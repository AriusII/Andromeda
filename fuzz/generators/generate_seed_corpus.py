#!/usr/bin/env python3
from __future__ import annotations

import argparse
import pathlib
import re
from typing import List


def load_targets(path: pathlib.Path) -> List[str]:
    text = path.read_text(encoding="utf-8")
    targets: List[str] = []
    for line in text.splitlines():
        m = re.match(r'\s*name\s*=\s*"([^"]+)"', line)
        if m:
            targets.append(m.group(1))
    return targets


def seed_payload(target: str) -> bytes:
    return f"WAVE14-SEED::{target}::v1".encode("utf-8")


def ensure_seed(root: pathlib.Path, target: str) -> pathlib.Path:
    corpus_dir = root / "fuzz" / "corpus" / target
    corpus_dir.mkdir(parents=True, exist_ok=True)
    seed_file = corpus_dir / "seed-basic.bin"
    if not seed_file.exists():
        seed_file.write_bytes(seed_payload(target))
    return seed_file


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate deterministic fuzz seed corpus")
    parser.add_argument("--ensure-only", action="store_true")
    parser.add_argument("--targets-file", default="fuzz/targets.toml")
    args = parser.parse_args()

    root = pathlib.Path.cwd()
    targets = load_targets(root / args.targets_file)
    if not targets:
        raise SystemExit("No targets found in fuzz/targets.toml")

    created = []
    for target in targets:
        created.append(ensure_seed(root, target))

    if not args.ensure_only:
        for item in created:
            print(item.as_posix())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
