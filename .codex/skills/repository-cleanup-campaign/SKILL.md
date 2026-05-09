---
name: repository-cleanup-campaign
description: "Use for broad cleanup campaigns across workspace."
category: rust-refactor
---

# repository-cleanup-campaign

## When to use
User asks cleanup/quality/refactor across repo.

## Purpose
Use for broad cleanup campaigns across workspace.

## Process
- Inventory code smells by category.
- Prioritize low-risk removals first.
- Separate analysis, cleanup, refactor and test repair workers.
- Consolidate results under `.work`.

## Expected output
- Cleanup campaign plan.

## Guardrails
- Do not turn cleanup into uncontrolled rewrite.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
