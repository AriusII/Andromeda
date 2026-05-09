---
name: rust-public-api-minimization
description: "Use to reduce unnecessary `pub` surface and stabilize APIs."
category: rust-refactor
---

# rust-public-api-minimization

## When to use
Public items may be overexposed.

## Purpose
Use to reduce unnecessary `pub` surface and stabilize APIs.

## Process
- Use private by default, then `pub(super)`, `pub(crate)`, `pub` only as needed.
- Check reexports, tests relying on internals, and semver implications.
- Document public APIs with invariants and errors.

## Expected output
- Public API reduction plan and compatibility risk.

## Guardrails
- Do not expose internals just for tests.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
