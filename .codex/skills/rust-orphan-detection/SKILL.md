---
name: rust-orphan-detection
description: "Use to find orphan files, modules, crates, features, examples, benches, scripts, or docs."
category: rust-refactor
---

# rust-orphan-detection

## When to use
Repository contains files or dependencies whose role is unclear.

## Purpose
Use to find orphan files, modules, crates, features, examples, benches, scripts, or docs.

## Process
- Compare filesystem with Cargo manifests and module declarations.
- Check features, benches, examples, scripts and docs references.
- Classify each orphan: remove, reconnect, archive, or document as temporary.

## Expected output
- Orphan inventory with action and evidence.

## Guardrails
- No silent orphan retention.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
