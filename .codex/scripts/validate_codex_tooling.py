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

required = [
    ROOT / "AGENTS.md",
    ROOT / ".codex" / "config.toml",
    ROOT / ".codex" / "hooks.json",
    ROOT / ".agents" / "skills",
]

for path in required:
    if not path.exists():
        errors.append(f"Missing required path: {path}")

# Validate hooks JSON.
hooks_path = ROOT / ".codex" / "hooks.json"
if hooks_path.exists():
    try:
        hooks = json.loads(hooks_path.read_text(encoding="utf-8"))
        if "hooks" not in hooks:
            errors.append("hooks.json must contain a top-level 'hooks' object.")
        required_hook_events = {
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PermissionRequest",
            "Stop",
        }
        configured_hook_events = set(hooks.get("hooks", {}))
        for missing_event in sorted(required_hook_events - configured_hook_events):
            errors.append(f"hooks.json must configure required event: {missing_event}")
        for event_name, event_hooks in hooks.get("hooks", {}).items():
            if not isinstance(event_hooks, list):
                errors.append(f"hooks.json event must be a list: {event_name}")
                continue
            for entry_index, entry in enumerate(event_hooks):
                for hook_index, hook in enumerate(entry.get("hooks", [])):
                    command = hook.get("command", "")
                    if not command:
                        errors.append(
                            f"hooks.json {event_name}[{entry_index}].hooks[{hook_index}] missing command"
                        )
                        continue
                    command_parts = command.split()
                    if len(command_parts) >= 2 and command_parts[1].startswith(".codex/"):
                        script_path = ROOT / command_parts[1]
                        if not script_path.exists():
                            errors.append(
                                f"hooks.json {event_name}[{entry_index}].hooks[{hook_index}] "
                                f"references missing script: {command_parts[1]}"
                            )
                    if "timeout" not in hook:
                        errors.append(
                            f"hooks.json {event_name}[{entry_index}].hooks[{hook_index}] "
                            "must set an explicit timeout"
                        )
                    if event_name in required_hook_events and not hook.get("statusMessage"):
                        errors.append(
                            f"hooks.json {event_name}[{entry_index}].hooks[{hook_index}] "
                            "must set statusMessage for transparent hook behavior"
                        )
    except Exception as exc:
        errors.append(f"Invalid hooks.json: {exc}")

# Validate Codex agent registry and agent TOML files.
config_path = ROOT / ".codex" / "config.toml"
agents_dir = ROOT / ".codex" / "agents"
skill_names: set[str] = set()
skills_dir = ROOT / ".agents" / "skills"
if skills_dir.exists():
    skill_names = {path.parent.name for path in skills_dir.glob("*/SKILL.md")}

if config_path.exists():
    try:
        config = tomllib.loads(config_path.read_text(encoding="utf-8"))
        configured_agents = config.get("agents", {})
        registered_agents = {
            agent_name
            for agent_name, value in configured_agents.items()
            if isinstance(value, dict) and "config_file" in value
        }
        parsed_agent_names: dict[str, Path] = {}
        if agents_dir.exists():
            for agent_toml in sorted(agents_dir.glob("*.toml")):
                try:
                    agent = tomllib.loads(agent_toml.read_text(encoding="utf-8"))
                except Exception as exc:
                    errors.append(f"{agent_toml}: invalid TOML: {exc}")
                    continue
                agent_name = agent.get("name")
                if not isinstance(agent_name, str) or not agent_name:
                    errors.append(f"{agent_toml}: missing name")
                    continue
                if agent_name in parsed_agent_names:
                    errors.append(
                        f"{agent_toml}: duplicate agent name {agent_name} "
                        f"also in {parsed_agent_names[agent_name]}"
                    )
                parsed_agent_names[agent_name] = agent_toml
                if not isinstance(agent.get("description"), str) or not agent["description"].strip():
                    errors.append(f"{agent_toml}: missing description")
                if (
                    not isinstance(agent.get("developer_instructions"), str)
                    or not agent["developer_instructions"].strip()
                ):
                    errors.append(f"{agent_toml}: missing developer_instructions")
                sandbox_mode = agent.get("sandbox_mode")
                role_text = f"{agent_name} {agent.get('description', '')}".lower()
                read_only_role_names = {
                    "andromeda-architect",
                    "documentation-consistency-editor",
                    "enterprise-readiness-agent",
                    "formal-invariants-agent",
                    "github-pr-reviewer",
                    "hardware-rust-performance-engineer",
                    "hooks-governance-auditor",
                    "map-analytics-summarizability-reviewer",
                    "modelization-catalog-governor",
                    "optimizer-statistics-critic",
                    "procedure-contract-reviewer",
                    "quic-rpc-contract-auditor",
                    "roadmap-scope-controller",
                    "rust-memory-safety-auditor",
                    "rust-unsafe-audit-agent",
                    "security-iam-auditor",
                    "security-iam-threat-modeler",
                    "source-grounding-researcher",
                    "srpl-language-specifier",
                    "storage-engine-page-layout-auditor",
                    "transaction-wal-recovery-auditor",
                }
                read_only_marker = re.search(
                    r"\b(?:auditor|reviewer|critic|guardian)\b|threat[- ]model|source-grounding",
                    role_text,
                )
                execution_oriented = {
                    "codex-skill-maintainer",
                    "devex-rust-tooling-agent",
                    "qa-crash-recovery-test-designer",
                    "test-verification-architect",
                }
                if agent_name in read_only_role_names or read_only_marker:
                    if agent_name not in execution_oriented and sandbox_mode != "read-only":
                        errors.append(
                            f"{agent_toml}: reviewer/auditor posture must be read-only"
                        )

                if "## Primary skills" in agent.get("developer_instructions", ""):
                    skill_section = agent["developer_instructions"].split("## Primary skills", 1)[1]
                    skill_section = skill_section.split("## ", 1)[0]
                    for skill_name in re.findall(r"-\s+`([-a-z0-9]+)`", skill_section):
                        if skill_name not in skill_names:
                            errors.append(
                                f"{agent_toml}: primary skill not found in .agents/skills: {skill_name}"
                            )

            missing_registry_entries = sorted(set(parsed_agent_names) - registered_agents)
            for agent_name in missing_registry_entries:
                errors.append(f".codex/config.toml does not register agent: {agent_name}")

        for agent_name in sorted(registered_agents):
            agent_config = configured_agents[agent_name]
            config_file = agent_config.get("config_file")
            if not isinstance(config_file, str) or not config_file:
                errors.append(f".codex/config.toml agent {agent_name}: missing config_file")
                continue
            agent_path = ROOT / ".codex" / config_file
            if not agent_path.exists():
                errors.append(f".codex/config.toml agent {agent_name}: missing {config_file}")
                continue
            try:
                agent = tomllib.loads(agent_path.read_text(encoding="utf-8"))
            except Exception as exc:
                errors.append(f".codex/config.toml agent {agent_name}: invalid TOML: {exc}")
                continue
            if agent.get("name") != agent_name:
                errors.append(
                    f".codex/config.toml agent {agent_name}: file declares name {agent.get('name')!r}"
                )
    except Exception as exc:
        errors.append(f"Invalid config.toml: {exc}")

# Validate skills.
if skills_dir.exists():
    seen_skills: dict[str, Path] = {}
    for skill_md in sorted(skills_dir.glob("*/SKILL.md")):
        text = skill_md.read_text(encoding="utf-8")
        if not text.startswith("---\n"):
            errors.append(f"{skill_md}: missing YAML frontmatter")
            continue
        parts = text.split("---", 2)
        if len(parts) < 3:
            errors.append(f"{skill_md}: malformed YAML frontmatter")
            continue
        fm = parts[1]
        if not re.search(r"^name:\s*[-a-z0-9]+\s*$", fm, re.M):
            errors.append(f"{skill_md}: missing or invalid name field")
        else:
            skill_name = re.search(r"^name:\s*([-a-z0-9]+)\s*$", fm, re.M).group(1)
            if skill_name != skill_md.parent.name:
                errors.append(
                    f"{skill_md}: name field must match folder name {skill_md.parent.name}"
                )
            if skill_name in seen_skills:
                errors.append(f"{skill_md}: duplicate skill name also in {seen_skills[skill_name]}")
            seen_skills[skill_name] = skill_md
        if not re.search(r"^description:\s*.+", fm, re.M):
            errors.append(f"{skill_md}: missing description field")
        if len(text.splitlines()) > 650:
            warnings.append(f"{skill_md}: long SKILL.md; consider moving details into references/")

if errors:
    print("Codex tooling validation failed:\n")
    for error in errors:
        print(f"ERROR: {error}")
    for warning in warnings:
        print(f"WARNING: {warning}")
    sys.exit(1)

print("Codex tooling validation passed.")
for warning in warnings:
    print(f"WARNING: {warning}")
