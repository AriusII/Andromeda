---
name: rust-file-splitting-placement
description: "Use to shrink or split large Rust files by responsibility."
category: rust-refactor
---

# rust-file-splitting-placement

## When to use
A file is large, mixed-responsibility, hard to test, or poorly placed.

## Purpose
Use to shrink or split large Rust files by responsibility.

## Process
- Identify distinct responsibilities and abstraction levels.
- Extract headers/codecs/validation/tests/effects into named modules.
- Keep `lib.rs` as a map and public surface only.
- Move tests near behavior or integration location as appropriate.

## Expected output
- Split plan with new module tree and validation.

## Guardrails
- No split named `part1`, `part2`, `misc`.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
