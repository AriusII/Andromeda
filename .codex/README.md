# Andromeda Codex Agents and Skills — Clean 2026 Package

This archive contains a `.codex`-only package for Andromeda Codex work.

## Scope

- Rust 1.95.0, Rust 2024 Edition, resolver 3.
- SGBDRT + SRPL mission-critical project.
- QUIC + custom Protobuf RPC.
- No gRPC native protocol.
- No JSON native protocol.
- Strong worker orchestration through `.work/codex/` reports.

## Contents

- `config.toml` — clean agent registry using `[agents.<name>]` tables.
- `agents/` — 13 strict agents.
- `skills/` — 56 focused skills.
- `routing/agent_skill_matrix.md` — recommended routing and skill mapping.
- `scripts/validate_codex_tooling.py` — local validation script.
- `context/andromeda-docs/` — copied Markdown/PDF project context.

## Install

Extract the archive at repository root so the repository contains `.codex/`.

Then run:

```bash
python .codex/scripts/validate_codex_tooling.py
python .codex/scripts/list_agents.py
python .codex/scripts/list_skills.py
```

PowerShell:

```powershell
python .codex/scripts/validate_codex_tooling.py
```
