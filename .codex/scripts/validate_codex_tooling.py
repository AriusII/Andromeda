#!/usr/bin/env python3
from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path.cwd()
errors: list[str] = []
warnings: list[str] = []

REQUIRED_PATHS = [
    ROOT / "AGENTS.md",
    ROOT / ".codex" / "config.toml",
    ROOT / ".codex" / "hooks.json",
    ROOT / ".codex" / "agents",
    ROOT / ".agents" / "skills",
]

REQUIRED_HOOK_EVENTS = {
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Stop",
}

READ_ONLY_ROLE_RE = re.compile(r"\b(?:auditor|reviewer|critic|guardian)\b|threat[- ]model|source-grounding", re.I)

def fail(message: str) -> None:
    errors.append(message)

def warn(message: str) -> None:
    warnings.append(message)

for path in REQUIRED_PATHS:
    if not path.exists():
        fail(f"Missing required path: {path}")

# Skills
skills_dir = ROOT / ".agents" / "skills"
skill_names: set[str] = set()
if skills_dir.exists():
    for skill_md in sorted(skills_dir.glob("*/SKILL.md")):
        rel = skill_md.relative_to(ROOT)
        text = skill_md.read_text(encoding="utf-8")
        if not text.startswith("---\n"):
            fail(f"{rel}: missing YAML frontmatter")
            continue
        parts = text.split("---", 2)
        if len(parts) < 3:
            fail(f"{rel}: malformed YAML frontmatter")
            continue
        fm = parts[1]
        name_match = re.search(r"^name:\s*([-a-z0-9]+)\s*$", fm, re.M)
        if not name_match:
            fail(f"{rel}: missing or invalid name field")
            continue
        name = name_match.group(1)
        if name != skill_md.parent.name:
            fail(f"{rel}: name field must match folder name")
        if name in skill_names:
            fail(f"{rel}: duplicate skill name {name}")
        skill_names.add(name)
        if not re.search(r"^description:\s*.+", fm, re.M):
            fail(f"{rel}: missing description field")
        if len(text.splitlines()) > 650:
            warn(f"{rel}: SKILL.md is long; consider moving details into references/")
    if len(skill_names) < 20:
        warn("Skill library has fewer than 20 skills; this package expects broad coverage.")

# Hooks
hooks_path = ROOT / ".codex" / "hooks.json"
if hooks_path.exists():
    try:
        hooks_doc = json.loads(hooks_path.read_text(encoding="utf-8"))
        hooks = hooks_doc.get("hooks")
        if not isinstance(hooks, dict):
            fail("hooks.json must contain a top-level hooks object")
        else:
            for event in sorted(REQUIRED_HOOK_EVENTS - set(hooks)):
                fail(f"hooks.json missing required event: {event}")
            for event, groups in hooks.items():
                if not isinstance(groups, list):
                    fail(f"hooks.json event {event} must be a list")
                    continue
                for group_index, group in enumerate(groups):
                    if not isinstance(group, dict):
                        fail(f"hooks.json {event}[{group_index}] must be an object")
                        continue
                    hook_list = group.get("hooks")
                    if not isinstance(hook_list, list) or not hook_list:
                        fail(f"hooks.json {event}[{group_index}].hooks must be a non-empty list")
                        continue
                    for hook_index, hook in enumerate(hook_list):
                        if not isinstance(hook, dict):
                            fail(f"hooks.json {event}[{group_index}].hooks[{hook_index}] must be an object")
                            continue
                        command = hook.get("command")
                        if not isinstance(command, str) or not command.strip():
                            fail(f"hooks.json {event}[{group_index}].hooks[{hook_index}] missing command")
                            continue
                        parts = command.split()
                        if len(parts) >= 2 and parts[1].startswith(".codex/"):
                            script = ROOT / parts[1]
                            if not script.exists():
                                fail(f"hooks.json {event}[{group_index}].hooks[{hook_index}] references missing script: {parts[1]}")
                        if "timeout" not in hook:
                            fail(f"hooks.json {event}[{group_index}].hooks[{hook_index}] missing timeout")
                        if not hook.get("statusMessage"):
                            fail(f"hooks.json {event}[{group_index}].hooks[{hook_index}] missing statusMessage")
    except Exception as exc:
        fail(f"Invalid hooks.json: {exc}")

# Agents and registry
config_path = ROOT / ".codex" / "config.toml"
agents_dir = ROOT / ".codex" / "agents"
if config_path.exists():
    try:
        config = tomllib.loads(config_path.read_text(encoding="utf-8"))
        registry = config.get("agents", {})
        if not isinstance(registry, dict):
            fail(".codex/config.toml [agents] must be a table")
            registry = {}
        registered = set(registry.keys())
        parsed = {}
        if agents_dir.exists():
            for agent_file in sorted(agents_dir.glob("*.toml")):
                rel = agent_file.relative_to(ROOT)
                try:
                    agent = tomllib.loads(agent_file.read_text(encoding="utf-8"))
                except Exception as exc:
                    fail(f"{rel}: invalid TOML: {exc}")
                    continue
                name = agent.get("name")
                if not isinstance(name, str) or not name:
                    fail(f"{rel}: missing name")
                    continue
                parsed[name] = agent_file
                if name not in registered:
                    fail(f"{rel}: agent is not registered in .codex/config.toml")
                if not isinstance(agent.get("description"), str) or not agent["description"].strip():
                    fail(f"{rel}: missing description")
                instructions = agent.get("developer_instructions")
                if not isinstance(instructions, str) or not instructions.strip():
                    fail(f"{rel}: missing developer_instructions")
                sandbox = agent.get("sandbox_mode")
                role_text = f"{name} {agent.get('description','')}"
                if READ_ONLY_ROLE_RE.search(role_text) and sandbox != "read-only":
                    fail(f"{rel}: auditor/reviewer/critic/threat-model/source-grounding agents must use sandbox_mode = read-only")
                if isinstance(instructions, str) and "## Primary skills" in instructions:
                    section = instructions.split("## Primary skills", 1)[1].split("## ", 1)[0]
                    for skill_name in re.findall(r"-\s+`([-a-z0-9]+)`", section):
                        if skill_name not in skill_names:
                            fail(f"{rel}: primary skill does not exist: {skill_name}")
        for name, entry in registry.items():
            if not isinstance(entry, dict):
                fail(f".codex/config.toml agent {name}: entry must be a table")
                continue
            config_file = entry.get("config_file")
            if not isinstance(config_file, str) or not config_file:
                fail(f".codex/config.toml agent {name}: missing config_file")
                continue
            path = ROOT / ".codex" / config_file
            if not path.exists():
                fail(f".codex/config.toml agent {name}: missing file {config_file}")
                continue
            try:
                agent = tomllib.loads(path.read_text(encoding="utf-8"))
                if agent.get("name") != name:
                    fail(f".codex/config.toml agent {name}: file declares {agent.get('name')!r}")
            except Exception as exc:
                fail(f".codex/config.toml agent {name}: invalid TOML: {exc}")
    except Exception as exc:
        fail(f"Invalid .codex/config.toml: {exc}")

if errors:
    print("Codex tooling validation failed:\n")
    for error in errors:
        print(f"ERROR: {error}")
    for warning in warnings:
        print(f"WARNING: {warning}")
    sys.exit(1)

print("Codex tooling validation passed.")
print(f"Skills: {len(skill_names)}")
if config_path.exists():
    config = tomllib.loads(config_path.read_text(encoding="utf-8"))
    print(f"Agents: {len(config.get('agents', {}))}")
for warning in warnings:
    print(f"WARNING: {warning}")
