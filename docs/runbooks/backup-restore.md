# Backup And Restore Runbook

## Purpose

Use this runbook to validate backup readiness, run restore or PITR drills, and
make retention decisions without weakening recovery truth.

## Prerequisites

- Backup manifest with base snapshot, required WAL range, catalog metadata,
  security metadata, format identity, and checksums.
- WAL archive coverage for the target restore or PITR window.
- Isolated restore target unless an approved disaster recovery procedure
  explicitly promotes the result.
- Retained audit, `RestoreTrace`, and `RecoveryReport` evidence paths.

## Procedure

1. Define the drill or incident: backup id, snapshot id, WAL range, target LSN
   or timestamp, RPO, RTO, and open-mode target.
2. Verify the manifest, snapshot, payload checksums, catalog metadata, security
   metadata, storage format, and audit evidence.
3. Validate the PITR window. Reject missing segments, duplicate segments, chain
   breaks, zero LSNs, end-before-start ranges, and targets outside the window.
4. Restore into an isolated target. Replay durable WAL only through the selected
   target and roll back incomplete transactions.
5. Validate storage, catalog, security, WAL, audit, and recovery invariants.
6. Open only in test or read-only mode until all validation passes and the
   disaster recovery authority approves production mode.
7. Record elapsed restore time, data-loss window, artifacts used, validation
   result, residual risk, and follow-up work.

## Retention Rules

- Do not delete snapshots, manifests, WAL archives, audit records, reports, or
  cluster evidence while backup, PITR, replica catch-up, MVCC, compliance, or
  forensic hold depends on them.
- Replicas are not backups.
- Audit evidence explains decisions; database truth comes from the latest valid
  snapshot plus durable WAL.

## Validation Commands

Use applicable owner tests when backup or restore code changes exist:

```powershell
cargo test -p andromeda-storage --test backup_physical_plan_contract --locked -- --nocapture
cargo test -p andromeda-storage --test backup_execution_plan --locked -- --nocapture
cargo test -p andromeda-storage --test restore_contract --locked -- --nocapture
cargo test -p andromeda-storage --test wal_gc_four_boundaries_integration --locked -- --nocapture
cargo test -p andromeda-audit --test hadr_backup_audit_contract --locked -- --nocapture
```

## Escalate When

- The restore target is outside the WAL archive range.
- The backup has never been restored and validated.
- Catalog, security, storage, WAL, audit, or recovery checks fail.
- Any retention deletion would remove the only restorable path.
