---
name: agent-skill-routing
description: "Use to choose between an agent, a skill, or direct work."
category: codex-tooling
---

# agent-skill-routing

## When to use
A request could use multiple agents or skills.

## Purpose
Use to choose between an agent, a skill, or direct work.

## Process
- Prefer a skill when the task is a repeatable micro-method.
- Use an orchestrator only for multi-worker coordination.
- Use a write worker only for bounded edits.
- Use a report-only worker for analysis and audit.

## Expected output
- Routing decision, selected skills, selected agent, reason, fallback.

## Guardrails
- Avoid creating new agents for narrow tasks.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
