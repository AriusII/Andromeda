---
name: rust-naming-noise-cleanup
description: "Use to clean Rust naming across crates, modules, files, functions and tests."
category: rust-refactor
---

# rust-naming-noise-cleanup

## When to use
Names include noise or unclear intent.

## Purpose
Use to clean Rust naming across crates, modules, files, functions and tests.

## Process
- Reject meaningless suffixes: v0, v1, phase, wave, worker, tmp, helper, misc, common unless justified.
- Use American English and domain vocabulary.
- Prefer precise names tied to responsibility and invariants.
- Check file location matches name and responsibility.

## Expected output
- Rename plan, impact, migration notes.

## Guardrails
- Do not rename public surface without compatibility review.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
