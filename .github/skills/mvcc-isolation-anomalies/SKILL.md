---
name: mvcc-isolation-anomalies
description: Analyzes MVCC visibility, isolation, anomalies, and long readers when prompts mention MVCC, snapshot, isolation, readers, writers, or anomalies.
license: MIT
---

# mvcc-isolation-anomalies

## When to use
- The prompt mentions MVCC, snapshot isolation, visibility, long reader, anomaly, write skew, phantom, lost update, or transaction state.
- A change touches andromeda-mvcc, transaction timestamps, storage visibility, or execution transaction scope.
- A test failure involves concurrent readers/writers or stale visibility.

## Purpose
Keep transaction visibility rules explicit and testable so concurrency behavior cannot accidentally weaken C5 correctness. The skill focuses on snapshots, write visibility, reader lifetimes, anomaly prevention, and their relationship to the transaction state machine.

## Process
1. Read transaction architecture and the transaction state-machine spec before deciding isolation behavior.
2. Name the isolation level or guarantee being implemented and list anomalies it permits or forbids.
3. Trace reader snapshot acquisition, writer commit visibility, rollback, garbage collection, and long-reader retention.
4. Ensure visibility decisions depend on durable transaction state, not transient memory-only markers.
5. Add deterministic concurrency tests for reader/writer ordering, rollback invisibility, long-reader safety, and anomaly cases.

## Expected output
- An MVCC visibility table for active, committed, aborted, recovered, and long-reader states.
- An anomaly checklist with expected behavior.
- Crate-owned deterministic tests or model/property tests to run.

## Reference docs
- `docs/architecture/TRANSACTION_ARCHITECTURE.md`
- `docs/specifications/SPEC_TRANSACTION_STATE_MACHINE_V0.md`

## Guardrails
- Do not make unflushed or aborted data visible.
- Do not resolve anomalies by weakening documented isolation silently.
- Do not rely on timing sleeps instead of deterministic ordering in tests.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
