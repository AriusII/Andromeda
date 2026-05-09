---
name: rust-toolchain-1950-policy
description: "Use to enforce Rust 1.95.0 / Edition 2024 / resolver 3 baseline."
category: rust-architecture
---

# rust-toolchain-1950-policy

## When to use
Toolchain, Cargo manifests, or MSRV policy are touched.

## Purpose
Use to enforce Rust 1.95.0 / Edition 2024 / resolver 3 baseline.

## Process
- Check rust-toolchain.toml, Cargo.toml edition/resolver/rust-version.
- Use stable for production; nightly only for selected tooling gates.
- Require ADR for MSRV changes.

## Expected output
- Toolchain policy findings and needed edits.

## Guardrails
- Do not silently bump MSRV.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
