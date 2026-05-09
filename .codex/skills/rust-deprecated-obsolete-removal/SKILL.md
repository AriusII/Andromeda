---
name: rust-deprecated-obsolete-removal
description: "Use to remove obsolete, deprecated or transitional code."
category: rust-refactor
---

# rust-deprecated-obsolete-removal

## When to use
Code, APIs, feature flags, comments or docs are obsolete.

## Purpose
Use to remove obsolete, deprecated or transitional code.

## Process
- Find deprecated annotations, TODOs, stale compatibility paths, old naming, unused feature flags.
- Check whether removal breaks contract or public API.
- Remove or create explicit migration note.

## Expected output
- Obsolete item list, removal safety, residual compatibility risks.

## Guardrails
- Do not preserve historical noise without owner or issue.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
