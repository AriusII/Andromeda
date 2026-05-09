---
name: dependency-supply-chain
description: "Use for Cargo dependency pruning and supply-chain governance."
category: rust-security
---

# dependency-supply-chain

## When to use
Dependencies, features, audits, or Cargo manifests are involved.

## Purpose
Use for Cargo dependency pruning and supply-chain governance.

## Process
- Check unused dependencies, duplicate versions, default features, license, advisories, unmaintained crates.
- Prefer minimal transitive surface for C5 crates.
- Use cargo tree, cargo audit, cargo deny, cargo vet when available.

## Expected output
- Dependency risk report and pruning plan.

## Guardrails
- Do not add crates for trivial functionality.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
