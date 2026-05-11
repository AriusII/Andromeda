---
name: hadr-backup-forensic
description: Guides HA/DR, quorum, fencing, backup, restore, PITR, and forensic startup when prompts mention cluster, failover, restore, PITR, or forensic mode.
license: MIT
---

# hadr-backup-forensic

## When to use
- The user mentions HA/DR, cluster, quorum, fencing, failover, replica lag, backup, restore, PITR, forensic startup, or disaster recovery.
- A change touches andromeda-hadr, backup, restore, recovery, manifest, WAL retention, or operations docs/tests.
- An incident analysis involves replica divergence, lag, or restoring a point in time.

## Purpose
Protect operational recovery paths by keeping cluster state, backups, restore points, and forensic startup consistent with durability and audit doctrine. The skill forces agents to think about quorum, fencing, PITR boundaries, and evidence before changing HA/DR or backup behavior.

## Process
1. Read HA/DR operations, backup/PITR operations, cluster manifest spec, forensic policy ADR, and relevant runbooks.
2. Classify the operation: normal backup, restore rehearsal, PITR, failover, fenced primary, replica catch-up, or forensic start.
3. Verify WAL, manifest, snapshot, audit, and catalog versions needed to prove the selected point or cluster member state.
4. Ensure fencing and quorum prevent split-brain and reject ambiguous primaries.
5. Produce validation evidence: restore drill, PITR boundary test, replica lag handling, and forensic read-only startup checks.

## Expected output
- An operations-safe sequence for backup, restore, PITR, failover, or forensic start.
- Required evidence artifacts and version boundaries.
- Risks, runbook steps, and tests before declaring operational readiness.

## Reference docs
- `docs/operations/HADR_AND_CLUSTER_OPERATIONS.md`
- `docs/operations/BACKUP_RESTORE_PITR.md`
- `docs/specifications/SPEC_HADR_CLUSTER_MANIFEST_V0.md`
- `docs/adr/ADR-0016-BACKUP_PITR_FORENSIC_POLICY.md`
- `docs/runbooks/RUNBOOK_BACKUP_RESTORE.md`
- `docs/runbooks/RUNBOOK_FORENSIC_START.md`
- `docs/runbooks/RUNBOOK_REPLICA_LAG.md`

## Guardrails
- No split-brain tolerance.
- No restore claim without WAL/manifest/catalog evidence.
- No forensic startup that mutates evidence.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
