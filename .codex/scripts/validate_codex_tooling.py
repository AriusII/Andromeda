#!/usr/bin/env python3
from __future__ import annotations
import json, re, sys, tomllib
from pathlib import Path

ROOT = Path.cwd()
CODEX = ROOT / ".codex"
errors: list[str] = []
warnings: list[str] = []

def fail(msg: str) -> None: errors.append(msg)
def warn(msg: str) -> None: warnings.append(msg)

required = [CODEX/'config.toml', CODEX/'agents', CODEX/'skills', CODEX/'routing']
for p in required:
    if not p.exists(): fail(f"Missing required path: {p}")

# Skills
skill_names: set[str] = set()
skills_dir = CODEX/'skills'
if skills_dir.exists():
    for skill_md in sorted(skills_dir.glob('*/SKILL.md')):
        rel = skill_md.relative_to(ROOT)
        text = skill_md.read_text(encoding='utf-8')
        if not text.startswith('---\n'):
            fail(f"{rel}: missing YAML frontmatter")
            continue
        parts = text.split('---', 2)
        if len(parts) < 3:
            fail(f"{rel}: malformed YAML frontmatter")
            continue
        fm = parts[1]
        m = re.search(r'^name:\s*([-a-z0-9]+)\s*$', fm, re.M)
        if not m:
            fail(f"{rel}: missing valid name field")
            continue
        name = m.group(1)
        if name != skill_md.parent.name:
            fail(f"{rel}: name field must match folder name")
        if name in skill_names:
            fail(f"{rel}: duplicate skill name")
        skill_names.add(name)
        if not re.search(r'^description:\s*.+', fm, re.M):
            fail(f"{rel}: missing description")
        for section in ['## When to use','## Process','## Expected output','## Guardrails']:
            if section not in text:
                fail(f"{rel}: missing section {section}")

# Config and agents
config_path = CODEX/'config.toml'
registered: set[str] = set()
if config_path.exists():
    try:
        config = tomllib.loads(config_path.read_text(encoding='utf-8'))
        agents = config.get('agents', {})
        if not isinstance(agents, dict):
            fail('.codex/config.toml [agents] must be a table')
            agents = {}
        for name, entry in agents.items():
            registered.add(name)
            if not isinstance(entry, dict):
                fail(f'.codex/config.toml agent {name}: entry must be a table')
                continue
            cf = entry.get('config_file')
            if not isinstance(cf, str) or not cf:
                fail(f'.codex/config.toml agent {name}: missing config_file')
                continue
            path = CODEX/cf
            if not path.exists():
                fail(f'.codex/config.toml agent {name}: missing file {cf}')
                continue
            try:
                doc = tomllib.loads(path.read_text(encoding='utf-8'))
            except Exception as exc:
                fail(f'{path.relative_to(ROOT)}: invalid TOML: {exc}')
                continue
            if doc.get('name') != name:
                fail(f'.codex/config.toml agent {name}: file declares {doc.get("name")!r}')
    except Exception as exc:
        fail(f'Invalid .codex/config.toml: {exc}')

agent_dir = CODEX/'agents'
if agent_dir.exists():
    for p in sorted(agent_dir.glob('*.toml')):
        rel = p.relative_to(ROOT)
        try:
            doc = tomllib.loads(p.read_text(encoding='utf-8'))
        except Exception as exc:
            fail(f'{rel}: invalid TOML: {exc}')
            continue
        name = doc.get('name')
        if not isinstance(name, str) or not name:
            fail(f'{rel}: missing name')
            continue
        if name not in registered:
            fail(f'{rel}: agent is not registered in .codex/config.toml')
        if not isinstance(doc.get('description'), str) or not doc['description'].strip():
            fail(f'{rel}: missing description')
        if not isinstance(doc.get('developer_instructions'), str) or not doc['developer_instructions'].strip():
            fail(f'{rel}: missing developer_instructions')
        skills = doc.get('primary_skills', [])
        if not isinstance(skills, list):
            fail(f'{rel}: primary_skills must be a list')
        else:
            for s in skills:
                if s not in skill_names:
                    fail(f'{rel}: missing referenced skill {s}')
        kind = doc.get('agent_kind')
        instr = doc.get('developer_instructions', '')
        if kind == 'readonly-worker-report-only':
            if '.work/codex' not in instr:
                fail(f'{rel}: read-only report worker must declare .work/codex report path')
            if doc.get('sandbox_mode') != 'workspace-write':
                warn(f'{rel}: report-only worker usually needs workspace-write to write its single .work report')

# Catalog JSON checks
for catalog_name in ['agent_catalog.json','skill_catalog.json']:
    p = CODEX/'routing'/catalog_name
    if not p.exists():
        fail(f'Missing routing catalog: {p}')
    else:
        try: json.loads(p.read_text(encoding='utf-8'))
        except Exception as exc: fail(f'{p.relative_to(ROOT)} invalid JSON: {exc}')

if len(skill_names) < 30:
    warn('Skill count is below 30; package may not provide the desired guardrail coverage.')

if errors:
    print('Codex tooling validation failed:\n')
    for e in errors: print('ERROR:', e)
    for w in warnings: print('WARNING:', w)
    sys.exit(1)
print('Codex tooling validation passed.')
print(f'Agents: {len(registered)}')
print(f'Skills: {len(skill_names)}')
for w in warnings: print('WARNING:', w)
