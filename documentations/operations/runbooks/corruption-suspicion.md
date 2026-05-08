# Corruption Suspicion Runbook

## Purpose

Use this runbook when WAL, page, segment, manifest, catalog, index, Map, audit, backup, or replica evidence suggests possible corruption.

## Scope

This runbook covers containment, ForensicStart, evidence preservation, consistency checks, and restore decision routing. It applies before normal application traffic is allowed to resume.

## Non-goals

This runbook does not repair corruption by overwriting evidence, replay past a corruption boundary, re-authorize application work during recovery, or treat audit replay as database truth.

## Prerequisites

- The operator can block application traffic or open only in SafeStart or ForensicStart.
- The operator can capture WAL, manifest, snapshot, page, catalog, audit, and cluster-state evidence.
- The operator has a restore/PITR route if corruption is confirmed or cannot be bounded.

## Status

| Area | Status | Evidence or limitation |
| --- | --- | --- |
| RecoveryReport shape | Contract preview documented for recovery validation. | `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`. |
| ForensicStart doctrine | Contract preview: application connections blocked and consistency report produced. | `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md`. |
| File WAL corruption boundary reporting | Implemented durable behavior where covered by file WAL report code and tests. | `crates/andromeda-storage/src/file_wal/report.rs`. |
| Full ForensicStart runtime orchestration | Planned gap unless a specific implementation path is proven. | This runbook must not claim a full production ForensicStart command as implemented. |

## Findings Ordered By Severity

| Severity | Finding | Exact invariant or contract affected | Concrete remediation |
| --- | --- | --- | --- |
| Critical | Suspected corruption must fail closed. | Invariant 10: no mission-critical behavior without crash/recovery validation. Quality gate: silent corruption blocks release. | Suspend writes, block application traffic, enter SafeStart or ForensicStart, and preserve evidence. |
| Critical | Replay past a corrupted WAL chain or accepting checksum mismatch can create silent divergence. | WAL/recovery quality gate; corruption model: stop replay at last valid record or enter ForensicStart/restore. | Stop at the corruption boundary, emit RecoveryReport or ForensicReport evidence, and route to restore if state cannot be proven safe. |
| High | Audit evidence helps explain decisions but is not database truth. | AuditLedger v0: audit replay is forensic only; transaction truth is latest valid cold snapshot plus durable WAL. | Use audit to explain decisions, not to reconstruct committed database state. |
| High | Replica evidence may be poisoned by the same logical corruption. | Backup is not HA/DR doctrine. | Use immutable backup/PITR validation for data repair, not a replica that already applied the bad change. |
| Medium | Index or Map divergence may be repairable without restoring the whole database if source tables are proven sane. | Storage corruption model: index rebuild or Map refresh is allowed only when source table evidence is valid. | Rebuild secondary structures only after source table, WAL, and catalog invariants pass. |

## Procedure

1. Contain the system.
   - Stop new writes on the affected database.
   - Block application traffic.
   - Keep Administration Surface and HA/DR Cluster Surface access restricted to incident operators.
   - Mark the database Suspended, SafeStart, or ForensicStart according to the available implementation path.

2. Preserve evidence.
   - Capture manifest chain, cold snapshot descriptor, WAL segment list, last valid WAL LSN, page or segment checksum failures, catalog version, audit decision records, and cluster membership state.
   - Do not compact, truncate, rewrite, or garbage collect the affected evidence until forensic hold is cleared.

3. Identify the corruption boundary.
   - For WAL, validate length, CRC/hash, predecessor chain, and gapless LSN order.
   - For pages, validate trailer guards, hash/checksum, and PageLsn expectations.
   - For cold segments, validate segment headers/trailers, block hashes, and manifest hashes.
   - For catalog, validate object versions, dependencies, and publication boundaries.
   - For indexes and Maps, compare against source tables only after source table health is proven.

4. Choose the containment outcome.
   - If corruption is not confirmed and all invariants pass, reopen through SafeStart criteria.
   - If corruption is bounded to rebuildable indexes or Maps, rebuild only the affected derived structures.
   - If database truth is not provable, keep application traffic blocked and use the restore/PITR drill runbook.

5. Produce incident evidence.
   - Produce a RecoveryReport or ForensicReport.
   - Include start mode, last valid WAL LSN, corruption boundary, skipped records, rebuilt structures, catalog/security status, open mode, warnings, and errors.
   - Preserve audit evidence for forensic and recovery decisions.

## Validation

Required before merge for runtime changes related to this runbook:

- WAL corruption tests for partial record, chain break, gap, duplicate LSN, and replay stop at last valid record.
- Page and segment checksum/hash tests.
- Manifest fallback tests.
- Catalog invariant validation tests.
- Index and Map rebuild tests that prove source table health first.
- ForensicStart tests proving application traffic is blocked and a consistency report is produced.
- Restore routing tests when corruption cannot be bounded.

Suggested commands when related code changes exist:

```powershell
cargo test -p andromeda-storage recovery
cargo test -p andromeda-storage file_wal
cargo test -p andromeda-catalog recovery
cargo test -p andromeda-observe audit
```

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Recovery wants to skip corrupted records and continue. | Replay boundary policy is unsafe or unclear. | Stop replay at the last valid record and require ForensicStart or restore decision. |
| Audit replay appears to reconstruct state. | Audit purpose is being confused with database truth. | Reword or fix the runtime path so audit remains forensic-only. |
| A replica looks healthy after suspected logical corruption. | Replica may have applied the same bad change. | Do not use it as backup; validate PITR from an earlier target. |

## References

- `AGENTS.md`
- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`
- `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md`
- `documentations/specs/AuditLedger_v0.md`
- `.agents/instructions/QUALITY_GATES.md`
- `crates/andromeda-storage/src/file_wal/report.rs`
