---
name: roadmap-intake-and-stage-selection
description: "Use to read a global roadmap and select the current detailed stage."
category: roadmap
---

# roadmap-intake-and-stage-selection

## When to use
A roadmap file and current stage file are provided.

## Purpose
Use to read a global roadmap and select the current detailed stage.

## Process
- Read global intent first.
- Identify current stage, prerequisites and excluded future work.
- Extract deliverables, acceptance criteria and dependencies.
- Prepare stage execution context for the roadmap orchestrator.

## Expected output
- Roadmap intent summary and active stage context.

## Guardrails
- Do not skip ahead without evidence.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
