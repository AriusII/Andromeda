---
name: code-review-professional-grade
description: "Use for rigorous code review of Rust/Andromeda changes."
category: rust-review
---

# code-review-professional-grade

## When to use
Review code for quality and professional standards.

## Purpose
Use for rigorous code review of Rust/Andromeda changes.

## Process
- Check correctness, architecture, naming, tests, error handling, observability, performance and security.
- Use severity and evidence.
- Prioritize actionable findings.
- Link findings to paths and invariants.

## Expected output
- Review report with severity and remediation.

## Guardrails
- No vague “looks good” review.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
