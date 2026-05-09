---
name: rust-allocation-clone-review
description: "Use to review allocations, clones, buffer growth and ownership pressure."
category: rust-performance
---

# rust-allocation-clone-review

## When to use
Hot path allocates or clones excessively.

## Purpose
Use to review allocations, clones, buffer growth and ownership pressure.

## Process
- Find unnecessary `String`, `Vec`, `clone`, boxing, Arc/Mutex and reallocations.
- Use capacity planning and borrowing where safe.
- Preserve clarity and lifetimes; do not overfit.

## Expected output
- Allocation findings and low-risk improvements.

## Guardrails
- Do not create brittle lifetime complexity for tiny gains.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
