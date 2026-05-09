---
name: mission-critical-release-gates
description: "Use for C4/C5 mission-critical readiness."
category: release
---

# mission-critical-release-gates

## When to use
Change affects WAL, recovery, security, storage, protocol, catalog or release.

## Purpose
Use for C4/C5 mission-critical readiness.

## Process
- Check invariant preservation.
- Check tests and recovery evidence.
- Check unsafe and dependency risk.
- Check observability and rollback plan.
- List blockers and go/no-go decision.

## Expected output
- Mission-critical gate report.

## Guardrails
- No release without backup/recovery evidence for C5 changes.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
