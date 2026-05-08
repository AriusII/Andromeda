# Restore and PITR Drill Runbook

## Purpose

Use this runbook to rehearse or validate restore and point-in-time recovery before a production incident requires it.

## Scope

This runbook covers backup artifact verification, immutable retention expectations, WAL archive range validation, target LSN selection, restore validation, and production opening criteria.

## Non-goals

This runbook does not declare a backup usable without a restore test. It does not treat replicas as backups. It does not publish restored state to production until catalog, security, storage, WAL, audit, and open-mode criteria are satisfied.

## Prerequisites

- A cold snapshot or backup artifact set is available.
- WAL archive range and target LSN or logical timestamp are known.
- Catalog version, Procedure contract metadata, security metadata, manifests, and audit ledger evidence are available.
- The drill target is isolated from production unless an approved disaster recovery procedure explicitly promotes it.
- The local backup/restore drill checker is understood as read-only. It does not restore data or replay WAL; it only inspects repository evidence.

## Status

| Area | Status | Evidence or limitation |
| --- | --- | --- |
| Backup physical artifacts and manifest validation | Implemented durable behavior where covered by backup artifact store, manifest format, and backup execution plan tests. | `crates/andromeda-storage/src/backup`; `crates/andromeda-storage/tests/backup_execution_plan`. |
| PITR window and target LSN validation | Implemented validation for WAL archive window and target LSN bounds. | `crates/andromeda-storage/src/backup/wal_archive_integration.rs`; backup physical plan tests. |
| Immutable backup retention | Contract preview. ColdStore immutability and backup artifact evidence exist, but physical immutability depends on storage policy and runtime integration. | `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`; `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md`. |
| Full production restore drill and cluster simulation | Planned gap. | `documentations/ROADMAP_IMPLEMENTATION_2026.md`. |

## Findings Ordered By Severity

| Severity | Finding | Exact invariant or contract affected | Concrete remediation |
| --- | --- | --- | --- |
| Critical | A backup that has not been restored and validated is only an assumption. | Invariant 10: no mission-critical behavior without crash/recovery validation. Restore/PITR doctrine: tested restore is mandatory. | Run scheduled restore drills that verify artifacts, replay WAL, validate invariants, and document opening criteria. |
| Critical | PITR must not replay outside the validated WAL archive window. | Backup/PITR contract: choose snapshot, choose WAL range, define restore target, validate production opening. | Reject target LSN before earliest restorable LSN or after latest restorable LSN. |
| Critical | Restored state must not open for production if catalog, security, storage, WAL, or audit invariants fail. | Invariants 2, 3, 6, 8, and 10; RecoveryReport and RestoreTrace contracts. | Open restored state only in test or read-only mode until all invariant checks pass. |
| High | Backup immutability must be policy-backed, not just documented. | Invariant 4: RAM/temp output is not truth. ColdStore and backup immutability contracts. | Use immutable retention controls, artifact checksums, manifest signatures or hashes, and forensic hold when required. |
| High | Audit ledger is forensic evidence, not database truth. | AuditLedger v0: replay is forensic-only and does not re-execute Procedures or make storage state visible. | Include audit in validation and incident explanation, but restore database truth from valid snapshot plus durable WAL. |

## Procedure

1. Define the drill.
   - Choose drill type: dry-run/verify-only, isolated read-only restore, isolated production-mode rehearsal, or approved disaster recovery restore.
   - Record RPO target, RTO target, backup id, snapshot id, WAL archive range, and target LSN or timestamp.

2. Verify backup artifacts.
   - Validate the backup manifest.
   - Validate cold snapshot descriptor, checksums or hashes, compatibility evidence, catalog metadata, security metadata, Procedure Store retention policy, manifests, WAL segment list, and audit ledger presence.
   - Confirm retention is immutable or record the immutability gap.

3. Validate the PITR window.
   - Compute earliest and latest restorable LSN from the finalized manifest.
   - Reject zero LSNs, end-before-start ranges, missing segments, chain breaks, duplicate segments, and incomplete coverage.
   - Reject target LSN outside the validated window.

4. Restore into an isolated target.
   - Restore the cold snapshot.
   - Replay WAL to the target LSN.
   - Roll back incomplete transactions.
   - Validate manifest, catalog, security, storage, index, Map, and audit invariants.
   - Produce RestoreTrace and RecoveryReport evidence.

5. Decide open mode.
   - Use test mode when validating the drill.
   - Use read-only mode when operators need inspection without production writes.
   - Use production mode only when all validation passes and the disaster recovery authority approves the opening.

6. Record drill outcome.
   - Record elapsed restore time against RTO.
   - Record data loss window against RPO.
   - Record failed checks, skipped records, target LSN, restored catalog version, and open mode.
   - Convert failures into remediation work before claiming backup readiness.

7. Run the local backup and restore drill check.
   - Execute the checker from the repository root.
   - Use JSON output when a local report, CI job, or release checklist needs structured evidence.
   - Treat a passing result as local artifact coverage only. It does not prove that a production restore can complete.
   - Convert missing tests, runbooks, or evidence commands into follow-up work before claiming restore readiness.

## Validation

For documentation-only changes to this runbook, validate with:

```powershell
git diff -- documentations/operations/runbooks/restore-pitr-drill.md documentations/operations/runbooks/index.md
rg -n "backup_restore_drill_check|RestoreTrace|RecoveryReport|PITR|WAL|Application Surface|ad hoc SQL|gRPC" documentations/operations/runbooks/restore-pitr-drill.md
python tools/testing/backup_restore_drill_check.py
python tools/testing/backup_restore_drill_check.py --json
```

Required before merge for runtime changes related to this runbook:

- Backup artifact manifest validation tests.
- WAL archive completeness and chain validation tests.
- PITR target LSN lower-bound and upper-bound rejection tests.
- Restore replay tests from base snapshot through target LSN.
- Incomplete transaction rollback tests.
- Catalog/security compatibility tests during restore.
- Audit ledger presence and forensic-only replay tests.
- Production opening gate tests that fail closed on any invariant violation.

Suggested commands when related code changes exist:

```powershell
cargo test -p andromeda-storage --test backup_physical_plan_contract --locked -- --nocapture
cargo test -p andromeda-storage --test backup_execution_plan --locked -- --nocapture
cargo test -p andromeda-storage --test restore_contract --locked -- --nocapture
cargo test -p andromeda-storage --test wal_gc_four_boundaries_integration --locked -- --nocapture
cargo test -p andromeda-observe --test hadr_backup_audit_contract --locked -- --nocapture
```

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Target LSN is outside the backup window. | Wrong backup, incomplete WAL archive, or incorrect recovery target. | Select a different backup or target LSN; do not force replay outside the window. |
| Manifest validates but catalog compatibility fails. | Backup and catalog contract versions drifted. | Keep restored target closed and escalate to catalog/storage owners. |
| Drill meets RPO but misses RTO. | Restore path is correct but too slow. | Treat as operational readiness risk and tune restore pipeline without weakening validation. |
| Backup artifacts lack immutable retention evidence. | Storage policy is incomplete or not captured. | Record planned gap, add immutable retention control, and repeat drill. |
| `backup_restore_drill_check.py` passes but no isolated restore transcript exists. | The local checker verifies repository evidence and documented commands only. | Keep production restore readiness blocked until an isolated drill records backup id, snapshot id, WAL range, target LSN, RestoreTrace, RecoveryReport, and opening decision. |
| `backup_restore_drill_check.py` reports a missing evidence command. | A runbook or validation matrix no longer records the local test target. | Restore the documented command only after confirming the crate test still exists, or record the missing target as a planned gap. |

## References

- `AGENTS.md`
- `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md`
- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`
- `documentations/ROADMAP_IMPLEMENTATION_2026.md`
- `documentations/specs/AuditLedger_v0.md`
- `.agents/registries/risk-register.yaml`
- `crates/andromeda-storage/src/backup/wal_archive_integration.rs`
