---
name: transaction-wal-recovery
description: Protects transaction kernel, WAL, recovery, durability, and crash behavior when prompts mention commit, rollback, WAL, recovery, checkpoint, or corruption.
license: MIT
---

# transaction-wal-recovery

## When to use
- The user mentions transaction, commit, rollback, WAL, fsync, recovery, checkpoint, crash, corruption, durability, or RecoveryReport.
- A change touches andromeda-transaction, andromeda-wal, recovery, manifest switching, or catalog/storage durability.
- A test or incident involves power loss, torn write, replay, or WAL pressure.

## Purpose
Maintain the C5 durability boundary: a commit is not visible until its WAL record is durable, recoverable, and tied to auditable recovery evidence. The skill forces crash-state thinking for transaction, WAL, checkpoint, manifest, and recovery changes.

## Process
1. Read transaction architecture plus WAL, segment, state-machine, recovery report, and crash test specs.
2. Enumerate crash points before validation, after WAL append, after flush, before visibility, after visibility, during checkpoint, and during manifest/root-pointer switch.
3. Require durable WAL-before-visible-commit and deterministic rollback/recovery semantics for every path.
4. Use runbooks to classify operational pressure, corruption suspicion, and recovery procedure impacts.
5. Add crash/recovery tests and verify reports prove what was recovered, rejected, truncated, or quarantined.

## Expected output
- A durability state machine and crash-point matrix.
- RecoveryReport expectations and operational runbook links.
- Exact tests or nextest profiles needed for WAL and recovery evidence.

## Reference docs
- `docs/architecture/TRANSACTION_ARCHITECTURE.md`
- `docs/adr/ADR-0005-WAL_DURABILITY_POLICY.md`
- `docs/specifications/SPEC_WAL_RECORD_V0.md`
- `docs/specifications/SPEC_FILE_WAL_SEGMENT_V0.md`
- `docs/specifications/SPEC_TRANSACTION_STATE_MACHINE_V0.md`
- `docs/specifications/SPEC_RECOVERY_REPORT_V0.md`
- `docs/specifications/SPEC_CRASH_RECOVERY_TEST_PLAN_V0.md`
- `docs/runbooks/RUNBOOK_RECOVERY.md`
- `docs/runbooks/RUNBOOK_WAL_PRESSURE.md`
- `docs/runbooks/RUNBOOK_CORRUPTION_SUSPICION.md`
- `docs/testing/CRASH_RECOVERY_TEST_PLAN.md`

## Guardrails
- No visible commit without durable WAL.
- No RAM-as-truth recovery shortcut.
- Do not hide crash flakiness with retries.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
