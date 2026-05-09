---
name: codex-orchestration-protocol
description: "Use when a request requires Codex to coordinate analysis workers, write workers, dependency queues, and final consolidation."
category: codex-orchestration
---

# codex-orchestration-protocol

## When to use
A user asks for broad refactor, implementation, roadmap execution, or multi-step work.

## Purpose
Use when a request requires Codex to coordinate analysis workers, write workers, dependency queues, and final consolidation.

## Process
- Normalize intent into a bounded mission statement.
- Divide work into analysis, planning, write execution, validation, and consolidation.
- Use read-only analysis workers before edits when scope, risk, or dependencies are unclear.
- Maintain a dependency queue; do not run workers that can collide on the same files.
- Write consolidated plans and final reports under `.work/codex/<task-slug>/`.

## Expected output
- Task slug, normalized scope, worker groups, dependency queue, validation plan, final consolidation path.

## Guardrails
- Do not spawn unnecessary agents. Prefer skills over new agents. Do not edit before scope is bounded.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
