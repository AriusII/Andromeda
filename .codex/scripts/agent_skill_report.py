#!/usr/bin/env python3
from __future__ import annotations
import json
from pathlib import Path

root = Path.cwd()
agent_catalog = json.loads((root / ".codex" / "routing" / "agent_catalog.json").read_text(encoding="utf-8"))
for agent in agent_catalog:
    print(f"\n## {agent['name']}")
    print(f"Sandbox: {agent['sandbox_mode']}")
    print("Skills:")
    for skill in agent["primary_skills"]:
        print(f"- {skill}")
