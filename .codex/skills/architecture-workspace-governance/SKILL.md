---
name: architecture-workspace-governance
description: "Use for Rust workspace, crates, folders, tests, docs and tooling architecture."
category: andromeda-architecture
---

# architecture-workspace-governance

## When to use
Architecture or repo layout is involved.

## Purpose
Use for Rust workspace, crates, folders, tests, docs and tooling architecture.

## Process
- Map crates to engine/plane responsibilities.
- Keep composition root thin.
- Keep skills and agents separated.
- Place tests and tooling in stable, documented locations.
- Preserve mission-critical validation gates.

## Expected output
- Workspace architecture plan.

## Guardrails
- No God Engine.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
