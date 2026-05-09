---
name: rust-crate-module-boundary
description: "Use to design or audit Cargo crate and module boundaries."
category: rust-architecture
---

# rust-crate-module-boundary

## When to use
Crate/module graph is unclear or drifting.

## Purpose
Use to design or audit Cargo crate and module boundaries.

## Process
- Map responsibility, dependency direction, public API, feature flags, and tests.
- Prevent cycles and `core`/`common` dumping grounds.
- Create contract crates only when they reduce conceptual coupling.

## Expected output
- Boundary review, dependency graph concerns, recommended crate/module changes.

## Guardrails
- Do not split crates merely to store files.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
