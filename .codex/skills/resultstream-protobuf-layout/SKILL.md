---
name: resultstream-protobuf-layout
description: "Use for ResultStream metadata/payload order and Protobuf layout."
category: andromeda-protocol
---

# resultstream-protobuf-layout

## When to use
Result streaming or StructuredObject transport is touched.

## Purpose
Use for ResultStream metadata/payload order and Protobuf layout.

## Process
- Ensure metadata precedes payload.
- Include contract hash, column descriptors, row count when contractually known, layout and batch descriptors.
- Bound batch sizes and validate before allocation.
- Support RowMajor, ColumnMajor, Hybrid policies.

## Expected output
- ResultStream layout plan and validation.

## Guardrails
- No payload before metadata.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
