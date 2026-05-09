---
name: transaction-wal-recovery
description: "Use for transaction kernel, WAL, recovery, durability and crash behavior."
category: andromeda-transaction
---

# transaction-wal-recovery

## When to use
Work touches commit, WAL, recovery, rollback, checkpoint or durability.

## Purpose
Use for transaction kernel, WAL, recovery, durability and crash behavior.

## Process
- Enforce commit visible = durable WAL.
- Use append-only segmented WAL with CRC/hash chain.
- Generate RecoveryReport after recovery.
- Add crash tests for before/after WAL flush and manifest switch.

## Expected output
- WAL/recovery plan, risk and tests.

## Guardrails
- No commit without durable WAL.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
