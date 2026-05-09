---
name: rust-fuzz-property-miri-loom
description: "Use for advanced Rust verification of parsers, codecs, unsafe and concurrency."
category: rust-testing
---

# rust-fuzz-property-miri-loom

## When to use
Work touches binary formats, unsafe, parser, RPC, WAL, or concurrency.

## Purpose
Use for advanced Rust verification of parsers, codecs, unsafe and concurrency.

## Process
- Use property tests for roundtrip/invariants.
- Use fuzzing for untrusted input parsers.
- Use Miri for unsafe-sensitive tests.
- Use Loom for small concurrency protocols only.

## Expected output
- Verification matrix and proposed harnesses.

## Guardrails
- Do not model whole engine with Loom.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
