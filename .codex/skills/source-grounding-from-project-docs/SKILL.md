---
name: source-grounding-from-project-docs
description: "Use to ground decisions in Andromeda project Markdown/PDF context."
category: codex-tooling
---

# source-grounding-from-project-docs

## When to use
Need to ensure an answer or design respects project doctrine.

## Purpose
Use to ground decisions in Andromeda project Markdown/PDF context.

## Process
- Read relevant documents from `.codex/context/andromeda-docs/` or repository docs.
- Prefer the six consolidated Markdown files for project doctrine.
- Use Rust doctrine document for Rust engineering rules.
- Use source citations or file paths in reports.

## Expected output
- Source-grounded findings with exact files and sections when possible.

## Guardrails
- Do not invent project doctrine. Mark uncertainty.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
