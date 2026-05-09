# Corruption Runbook

## Purpose

Use this runbook when WAL, page, segment, manifest, catalog, index, audit,
backup, or replica evidence suggests corruption.

## Prerequisites

- Operators can block application traffic.
- Durable artifacts can be held: WAL, manifests, snapshots, pages, catalog
  metadata, audit records, cluster state, and backup manifests.
- Restore/PITR routing exists if local truth cannot be proven.

## Procedure

1. Fail closed. Stop new writes and block Application Surface traffic for the
   affected database.
2. Place forensic hold on WAL, audit, snapshots, manifests, reports, backup
   artifacts, and cluster evidence.
3. Capture the first observed symptom, last known good LSN, candidate corruption
   boundary, catalog/security status, and impacted replicas.
4. Validate WAL length, checksum, predecessor chain, and gapless LSN ordering.
5. Validate page trailers, checksums, page LSNs, segment headers, manifest
   hashes, catalog versions, and security policy status.
6. Classify the boundary as clean, tail truncation, middle corruption, unknown
   format, catalog mismatch, derived-index divergence, or unbounded.
7. Use SafeStart only when all required invariants pass. Use ForensicStart or
   RefuseStart when evidence is unsafe or incomplete.
8. Rebuild derived structures only after source table, catalog, and WAL truth is
   proven.
9. Route to backup/restore or PITR when database truth is not provable.
10. Produce `RecoveryReport` or forensic evidence before reopening.

## Validation Commands

```powershell
cargo test -p andromeda-recovery --test file_wal_recovery_contract --locked -- --nocapture
cargo test -p andromeda-storage --test wal_scan_recovery_contract --locked -- --nocapture
cargo test -p andromeda-recovery --test recovery_completeness_contract --locked -- --nocapture
cargo test -p andromeda-storage --locked forensic_start -- --nocapture
```

## Escalate When

- Recovery would skip corrupted records and continue as normal truth.
- Audit replay is being treated as database reconstruction.
- Replica evidence may have applied the same logical corruption.
- Application traffic can inspect raw WAL, raw pages, or forensic controls.
