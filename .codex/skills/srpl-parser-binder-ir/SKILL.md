---
name: srpl-parser-binder-ir
description: "Use for SRPL parser, binder, diagnostics, semantic IR/ALT and plan preparation."
category: andromeda-srpl
---

# srpl-parser-binder-ir

## When to use
Parser/binder/IR implementation or review.

## Purpose
Use for SRPL parser, binder, diagnostics, semantic IR/ALT and plan preparation.

## Process
- Separate parser, binder, semantic validation and IR.
- Resolve names, types, cardinality, effects and policies statically.
- Use stable diagnostic codes.
- Ensure IR hash is formatting-insensitive.

## Expected output
- Parser/binder/IR plan and tests.

## Guardrails
- Do not let source text formatting affect semantic identity.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
