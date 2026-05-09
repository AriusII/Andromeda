---
name: codex-agent-authoring
description: "Use to author or revise Codex agent TOML files for Andromeda."
category: codex-tooling
---

# codex-agent-authoring

## When to use
Need to create/update .codex/agents/*.toml.

## Purpose
Use to author or revise Codex agent TOML files for Andromeda.

## Process
- Give each agent one mission and a strict output contract.
- Keep the registry small.
- Use `developer_instructions` for behavior, not duplicated skill bodies.
- Attach primary skills by name.
- Register every agent in `.codex/config.toml` as `[agents.<name>]` table.

## Expected output
- Agent TOML, registry entry, skills list, validation notes.

## Guardrails
- No flat scalar entries under `[agents]`. No duplicate agent purposes.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
