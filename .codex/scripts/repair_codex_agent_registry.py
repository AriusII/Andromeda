#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path
import json
import re
import tomllib

ROOT = Path.cwd()
CODEX_DIR = ROOT / ".codex"
CONFIG = CODEX_DIR / "config.toml"
AGENTS_DIR = CODEX_DIR / "agents"
AGENT_CATALOG = CODEX_DIR / "routing" / "agent_catalog.json"

TOP_LEVEL_KEYS = [
    "model",
    "model_reasoning_effort",
    "model_verbosity",
    "sandbox_mode",
    "include_environment_context",
    "include_permissions_instructions",
    "approval_policy",
    "commit_attribution",
]
RUNTIME_KEYS = {
    "max_threads",
    "max_depth",
    "job_max_runtime_seconds",
    "interrupt_message",
}
READ_ONLY_NAMES = {
    "analytics-gpu-architect",
    "andromeda-architect",
    "andromeda-chief-architect",
    "benchmark-evidence-architect",
    "catalog-contract-engine-architect",
    "doctrine-guardian",
    "hadr-backup-architect",
    "hadr-operations-runbook-agent",
    "observability-forensic-architect",
    "optimizer-statistics-architect",
    "performance-engineer",
    "protobuf-contract-architect",
    "quic-transport-architect",
    "research-synthesizer",
    "rust-workspace-architect",
    "srpl-compiler-ir-architect",
    "srpl-language-designer",
    "storage-engine-architect",
    "transaction-kernel-architect",
    "type-system-architect",
    "wal-recovery-specialist",
}
WORKSPACE_WRITE_NAMES = {
    "binary-format-specifier",
    "documentation-architect",
    "promptops-curator-agent",
    "release-governance-agent",
    "risk-decision-manager",
}
READ_ONLY_ROLE_RE = re.compile(
    r"\b(?:auditor|reviewer|critic|guardian)\b|threat[- ]model|source-grounding",
    re.I,
)


def quote(value: object) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return repr(value)
    if isinstance(value, list):
        return "[" + ", ".join(quote(v) for v in value) + "]"
    s = str(value).replace("\\", "\\\\").replace('"', '\\"')
    return f'"{s}"'


def toml_key(key: str) -> str:
    if re.fullmatch(r"[A-Za-z0-9_-]+", key):
        return key
    return quote(key)


def sanitize_description(text: str) -> str:
    return " ".join(text.strip().split()).replace('"', "'")


def load_config() -> dict[str, object]:
    if not CONFIG.exists():
        return {}
    return tomllib.loads(CONFIG.read_text(encoding="utf-8"))


def parse_agent_file(path: Path) -> dict[str, object]:
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    name = data.get("name")
    if not isinstance(name, str) or not name.strip():
        data["name"] = path.stem
    description = data.get("description")
    if not isinstance(description, str) or not description.strip():
        instructions = data.get("developer_instructions")
        if isinstance(instructions, str) and instructions.strip():
            first = next(
                (line.strip("# ").strip() for line in instructions.splitlines() if line.strip()),
                "Codex agent",
            )
            data["description"] = first
        else:
            data["description"] = f"Codex agent for {data['name'].replace('-', ' ')}."
    return data


def intended_sandbox(name: str, description: str) -> str:
    if name in WORKSPACE_WRITE_NAMES:
        return "workspace-write"
    if name in READ_ONLY_NAMES or READ_ONLY_ROLE_RE.search(f"{name} {description}"):
        return "read-only"
    if any(token in name for token in ("implementer", "maintainer", "engineer", "curator")):
        return "workspace-write"
    if "architect" in name:
        return "read-only"
    return "workspace-write"


def replace_or_insert_scalar(text: str, key: str, value: str) -> str:
    line = f'{key} = "{value}"'
    pattern = re.compile(rf"^{re.escape(key)}\s*=.*$", re.M)
    if pattern.search(text):
        return pattern.sub(line, text, count=1)

    insert_after = None
    for candidate in ("description", "sandbox_mode", "name"):
        candidate_pattern = re.compile(rf"^{re.escape(candidate)}\s*=.*$", re.M)
        match = candidate_pattern.search(text)
        if match:
            insert_after = match
            if candidate == "description":
                break

    if insert_after is None:
        return line + "\n" + text
    return text[: insert_after.end()] + "\n" + line + text[insert_after.end() :]


def normalize_agent_file(path: Path) -> bool:
    text = path.read_text(encoding="utf-8")
    data = parse_agent_file(path)
    name = str(data["name"])
    description = str(data["description"])

    updated = text
    if not isinstance(data.get("sandbox_mode"), str) or not data.get("sandbox_mode"):
        updated = replace_or_insert_scalar(updated, "sandbox_mode", intended_sandbox(name, description))

    if updated != text:
        path.write_text(updated, encoding="utf-8")
        return True
    return False


def extract_primary_skills(agent: dict[str, object]) -> list[str]:
    skills = agent.get("primary_skills")
    if isinstance(skills, list) and all(isinstance(skill, str) for skill in skills):
        return list(skills)

    instructions = agent.get("developer_instructions")
    if not isinstance(instructions, str) or "## Primary skills" not in instructions:
        return []
    section = instructions.split("## Primary skills", 1)[1].split("## ", 1)[0]
    return re.findall(r"-\s+`([-a-z0-9]+)`", section)


def collect_agent_entries() -> list[tuple[str, str, str, dict[str, object]]]:
    entries: list[tuple[str, str, str, dict[str, object]]] = []
    seen: set[str] = set()
    for agent_file in sorted(AGENTS_DIR.glob("*.toml")):
        agent = parse_agent_file(agent_file)
        name = str(agent["name"]).strip()
        if name in seen:
            raise SystemExit(f"Duplicate agent name discovered: {name}")
        seen.add(name)
        rel = agent_file.relative_to(CODEX_DIR).as_posix()
        description = sanitize_description(str(agent["description"]))
        entries.append((name, rel, description, agent))
    return entries


def emit_table(lines: list[str], table_name: str, table: dict[str, object]) -> None:
    scalar_items = [(key, value) for key, value in table.items() if not isinstance(value, dict)]
    nested_items = [(key, value) for key, value in table.items() if isinstance(value, dict)]

    if scalar_items:
        lines.append(f"[{table_name}]")
        for key, value in sorted(scalar_items):
            lines.append(f"{toml_key(key)} = {quote(value)}")
        lines.append("")

    for key, value in sorted(nested_items):
        emit_table(lines, f"{table_name}.{toml_key(key)}", value)


def build_config(original: dict[str, object], entries: list[tuple[str, str, str, dict[str, object]]]) -> str:
    header = {key: original[key] for key in TOP_LEVEL_KEYS if key in original}
    if not header:
        header = {
            "sandbox_mode": "workspace-write",
            "approval_policy": "on-request",
        }

    agent_runtime: dict[str, object] = {}
    for source_name in ("agent_runtime", "agent_defaults"):
        source = original.get(source_name)
        if isinstance(source, dict):
            for key, value in source.items():
                if key in RUNTIME_KEYS:
                    agent_runtime[key] = value

    old_agents = original.get("agents")
    if isinstance(old_agents, dict):
        for key, value in old_agents.items():
            if key in RUNTIME_KEYS and not isinstance(value, dict):
                agent_runtime[key] = value

    lines: list[str] = [
        "# Andromeda Codex configuration",
        "# Regenerated by .codex/scripts/repair_codex_agent_registry.py",
    ]
    for key in TOP_LEVEL_KEYS:
        if key in header:
            lines.append(f"{key} = {quote(header[key])}")
    lines.append("")

    for section_name in ("features",):
        section = original.get(section_name)
        if isinstance(section, dict) and section:
            emit_table(lines, section_name, section)

    if agent_runtime:
        emit_table(lines, "agent_runtime", agent_runtime)

    for section_name in ("tui", "mcp_servers"):
        section = original.get(section_name)
        if isinstance(section, dict) and section:
            emit_table(lines, section_name, section)

    lines.append("# Agent registry")
    for name, rel, description, _agent in entries:
        lines.append(f"[agents.{name}]")
        lines.append(f"config_file = {quote(rel)}")
        lines.append(f"description = {quote(description)}")
        lines.append("")

    return "\n".join(lines).rstrip() + "\n"


def build_agent_catalog(entries: list[tuple[str, str, str, dict[str, object]]]) -> str:
    catalog = []
    for name, _rel, description, agent in entries:
        catalog.append(
            {
                "name": name,
                "description": description,
                "sandbox_mode": agent.get("sandbox_mode", intended_sandbox(name, description)),
                "primary_skills": extract_primary_skills(agent),
            }
        )
    return json.dumps(catalog, indent=2) + "\n"


def main() -> int:
    if not AGENTS_DIR.exists():
        raise SystemExit("Missing .codex/agents directory")

    normalized_count = 0
    for agent_file in sorted(AGENTS_DIR.glob("*.toml")):
        if normalize_agent_file(agent_file):
            normalized_count += 1

    original = load_config()
    entries = collect_agent_entries()

    CONFIG.write_text(build_config(original, entries), encoding="utf-8")
    AGENT_CATALOG.write_text(build_agent_catalog(entries), encoding="utf-8")

    print(f"Normalized {normalized_count} agent files.")
    print(f"Regenerated {CONFIG} with {len(entries)} registered agents.")
    print(f"Regenerated {AGENT_CATALOG} with {len(entries)} catalog entries.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
