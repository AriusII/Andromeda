# Backup Retention Runbook

## Purpose

Use this runbook to define, verify, and enforce backup, WAL archive, restore, audit, and forensic retention boundaries without weakening Andromeda recovery truth.

## Scope

This runbook covers backup artifact retention, immutable cold snapshot expectations, WAL archive retention, PITR window protection, audit-ledger retention evidence, forensic hold, restore drill evidence, retention cleanup, and deletion gating.

## Non-goals

This runbook does not choose legal retention periods, define storage-provider controls, create a new backup format, create new CLI syntax, or claim that replicas are backups. It does not permit deletion of WAL, snapshots, manifests, audit evidence, or reports while restore, replica, MVCC, backup/PITR, or forensic gates still need them.

## Prerequisites

- A documented retention policy version exists for the environment.
- Backup manifests identify the base snapshot, required WAL range, catalog/security metadata, storage format identity, and artifact checksums or hashes.
- WAL archive coverage is known for each backup and PITR window.
- Immutable storage controls or compensating evidence are available and recorded.
- Audit-ledger retention compaction rules are understood: retained record evidence persists, but physical journal bytes may be compacted under policy.
- Operators can place and release forensic hold without using the Application Surface.
- Restore drill evidence exists or the backup readiness status is explicitly marked as unproven.

## Application Surface Separation

Backup retention is not an Application Surface operation. The Application Surface must reject backup, backup validation, retention, restore, PITR, forensic hold, raw artifact inspection, and deletion control operations with a typed denial and `NoTransaction` effect.

Retention policy changes, hold placement, hold release, backup validation, and deletion approval must use the Administration Surface or BackupAgent operation family. HA/DR-related WAL retention decisions may also require HA/DR Cluster Surface evidence. Do not expose retention controls through application Procedures, application RPC, generic command tunnels, or ad hoc SQL.

## Status

| Area | Status | Evidence or limitation |
| --- | --- | --- |
| Backup and PITR validation gates | Implemented durable behavior where covered by backup physical plan, backup execution plan, and restore contract tests. | `documentations/testing/step-11-validation-matrix.md`. |
| Immutable backup retention | Contract preview. | ColdStore immutability and backup artifact expectations are documented, but physical immutability depends on storage policy and runtime integration. |
| Audit retention compaction | Contract preview and implementation evidence where covered by AuditLedger contracts. | `documentations/specs/AuditLedger_v0.md`; `documentations/governance/decisions/DEC-033-durable-audit-ledger.md`. |
| Full backup/restore drill evidence | Planned gap until a release record proves exact backup, WAL archive, target LSN, restore validation, and opening criteria. | `documentations/testing/step-11-validation-matrix.md`. |
| Retention deletion automation | Planned gap unless implementation evidence proves deletion gates, audit evidence, and hold enforcement. | Treat deletion as operator-approved until proven otherwise. |

## Retention Classes

| Artifact class | Why it is retained | Release condition |
| --- | --- | --- |
| Backup manifest | Binds backup id, base snapshot, WAL range, checksums, catalog/security metadata, and restore inputs. | Retention policy expired, no forensic hold, no compliance hold, and successor restore evidence is accepted. |
| Cold snapshot and backup payload | Provides the base durable state for restore. | Retention policy expired and no active restore, PITR, forensic, or compliance dependency exists. |
| WAL archive | Provides PITR coverage and replay from the base snapshot. | All dependent PITR windows, replica lag windows, backup windows, MVCC pins, and forensic holds are cleared. |
| Cluster manifest and HA/DR evidence | Explains failover, promotion, epoch, quorum, and fencing decisions. | HA/DR retention policy expired and no incident, audit, or restore dependency remains. |
| Audit ledger records | Preserve critical security, admin, HA/DR, backup, restore, forensic, and recovery decisions. | Audit retention policy permits compaction or deletion and retained payload checksum evidence remains valid. |
| Recovery, Restore, and Forensic reports | Prove startup, restore, corruption, and opening decisions. | Incident and compliance owners approve release, and linked artifacts remain sufficient for required evidence. |
| Forensic hold bundle | Prevents cleanup during investigation. | Incident authority releases the hold after evidence inventory and restore/PITR implications are validated. |

## Findings Ordered By Severity

| Severity | Finding | Exact invariant or contract affected | Concrete remediation |
| --- | --- | --- | --- |
| Critical | Deleting WAL before all backup, PITR, replica, MVCC, and forensic dependencies clear can make recovery impossible. | Canonical truth is latest valid cold snapshot plus durable WAL; backup/PITR retention boundary. | Require a deletion preflight that proves every retention dependency is closed. |
| Critical | A backup without restore validation is not a recovery guarantee. | Invariant 10 and backup/PITR drill doctrine. | Keep readiness unproven until an isolated restore validates manifest, snapshot, WAL range, target LSN, catalog, security, storage, and opening criteria. |
| Critical | Retention controls must not be operated from the Application Surface. | Invariant 8 and SecurityAdmission surface classification. | Route retention controls through Administration Surface or BackupAgent only and preserve application-denial evidence. |
| High | Audit compaction can be misread as byte-for-byte immutability. | AuditLedger retention compaction caveat. | Preserve retained payload checksum and policy evidence, but do not claim physical journal bytes remain immutable across compaction. |
| High | Forensic hold overrides ordinary expiration. | Forensic decision and recovery evidence retention. | Block cleanup while a forensic hold references the artifact or any dependent evidence chain. |

## Evidence Artifacts

Retain the following artifacts for every retention cycle:

| Artifact | Required contents |
| --- | --- |
| Retention policy version | Policy id, owner, effective time, artifact classes, expiration rules, hold precedence, and approval authority. |
| Backup inventory | Backup id, manifest id, base snapshot id, WAL archive start, WAL archive end, catalog/security metadata, and checksum or hash status. |
| PITR window report | Earliest restorable LSN, latest restorable LSN, target coverage, missing segments, and chain validation result. |
| Immutability evidence | Storage control status, object lock or equivalent evidence when available, manifest checksum, and limitation notes. |
| Audit evidence | Backup decision ids, restore decision ids, retention decision ids, forensic hold ids, and deletion approval ids. |
| Restore drill evidence | Restore target, target LSN or timestamp, validation policy, result, `RestoreTrace`, and `RecoveryReport`. |
| Delete candidate report | Candidate artifacts, dependency checks, hold checks, last restorable point after deletion, and operator approval. |
| Exception report | Expired but retained artifacts, reason, owner, review date, and residual risk. |

## Procedure

1. Identify the retention scope.
   - Select the database, cluster, backup set, environment, and policy version.
   - Record whether this is a routine retention cycle, incident hold, compliance hold, restore rehearsal, or disaster recovery event.

2. Build the artifact inventory.
   - List backup manifests, cold snapshots, backup payloads, WAL archive segments, audit ledger ranges, restore reports, recovery reports, forensic reports, and cluster manifests.
   - Bind each backup to its base snapshot and required WAL range.
   - Record missing, duplicate, corrupt, or unverified artifacts as findings.

3. Validate backup and WAL coverage.
   - Validate manifest identity and checksums or hashes.
   - Validate WAL archive start and end boundaries.
   - Reject end-before-start ranges, gaps, duplicate segments, chain breaks, missing PITR target coverage, and unknown format evidence.
   - Record earliest and latest restorable LSN for each backup.

4. Apply retention pins.
   - Pin artifacts required by active backup windows, PITR windows, replica catch-up, MVCC snapshots, restore drills, compliance policy, and forensic hold.
   - Pin audit evidence needed to explain backup, restore, retention, recovery, HA/DR, and forensic decisions.
   - Record every pin with owner, reason, start time, expected review time, and release authority.

5. Enforce immutability or compensating controls.
   - Use immutable storage controls where available.
   - Record storage policy evidence, object lock evidence, or equivalent protection.
   - When physical immutability is not proven, mark the backup readiness gap and increase restore drill priority.

6. Preflight deletion candidates.
   - Select only artifacts whose retention policy has expired.
   - Prove no active backup, PITR, replica, MVCC, restore, compliance, or forensic dependency remains.
   - Prove deletion does not break the latest required restore path or the oldest still-required PITR target.
   - Require audit evidence and operator approval before deletion.

7. Execute cleanup through the approved surface.
   - Use Administration Surface or BackupAgent controls only.
   - Keep Application Surface rejection evidence for any attempted application-side retention control.
   - Record deleted artifact ids, retained artifact ids, checksums, policy version, approval id, and completion status.

8. Validate after cleanup.
   - Recompute backup inventory and PITR window boundaries.
   - Verify retained manifests, snapshots, WAL archives, audit records, and reports still satisfy policy.
   - Run or schedule a restore drill for the oldest retained backup and a recent PITR target.
   - Convert any failed validation into remediation work before claiming retention health.

9. Run the local backup and restore drill check.
   - Execute the read-only checker from the repository root.
   - Use JSON output when a local report, CI job, or release checklist needs structured evidence.
   - Treat a passing result as local artifact coverage only. It does not prove that a production restore can complete.
   - Convert missing tests, runbooks, or evidence commands into follow-up work before deleting retained artifacts.

## Validation

For documentation-only changes to this runbook, validate with:

```powershell
git diff -- documentations/operations/runbooks/backup-retention.md documentations/operations/runbooks/index.md
rg -n "Application Surface|BackupAgent|retention|forensic hold|WAL archive|PITR|AuditLedger|ad hoc SQL|gRPC" documentations/operations/runbooks/backup-retention.md
python tools/testing/backup_restore_drill_check.py
python tools/testing/backup_restore_drill_check.py --json
```

Required before accepting runtime work related to this runbook:

- Backup manifest validation tests.
- WAL archive completeness, chain, and PITR boundary tests.
- Retention dependency tests covering backups, replicas, MVCC pins, PITR windows, and forensic holds.
- Deletion preflight tests that reject active dependencies and missing audit evidence.
- Audit-ledger retention compaction tests preserving retained payload checksum evidence.
- Application Surface denial tests for backup, retention, restore, PITR, and forensic hold controls.
- Restore drill tests proving retained artifacts can restore to an explicit target LSN.

Suggested commands when related code changes exist:

```powershell
cargo test -p andromeda-storage --test backup_physical_plan_contract --locked -- --nocapture
cargo test -p andromeda-storage --test backup_execution_plan --locked -- --nocapture
cargo test -p andromeda-storage --test restore_contract --locked -- --nocapture
cargo test -p andromeda-storage --test wal_gc_four_boundaries_integration --locked -- --nocapture
cargo test -p andromeda-observe --test durable_audit_sink_contract --locked -- --nocapture
cargo test -p andromeda-observe --test hadr_backup_audit_contract --locked -- --nocapture
```

## Rollback

| Phase | Rollback or containment action |
| --- | --- |
| Before deletion approval | Cancel cleanup, keep all artifacts pinned, and record the rejected candidate set. |
| After policy metadata error | Publish a corrected retention policy version, supersede the flawed decision with audit evidence, and re-run deletion preflight. |
| After deletion is queued but not completed | Stop the deletion job, preserve partial job evidence, and re-inventory affected artifacts. |
| After artifact deletion | Treat missing artifacts as an incident. Validate remaining backups and PITR windows, update RPO risk, and restore from an older valid backup if needed. Deleted durable artifacts must not be described as recoverable unless independent evidence proves that. |
| After forensic hold was released incorrectly | Reapply hold to remaining artifacts, preserve the mistaken release evidence, and escalate to incident authority. |

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| WAL archive deletion would break a PITR target. | Retention preflight ignored restore boundaries. | Reject deletion and pin the required WAL range. |
| Backup manifest is retained but payload is missing. | Inventory or storage lifecycle drift. | Mark backup unusable, validate alternate backup, and investigate deletion evidence. |
| Audit compaction appears to change journal bytes. | Policy-governed compaction rethreaded retained records. | Verify retained payload checksum evidence and do not claim byte-for-byte immutability across compaction. |
| Application users can start retention cleanup. | Surface separation drift. | Reject the route on Application Surface and require Administration or BackupAgent admission. |
| Forensic hold blocks all cleanup. | Hold scope is broad or stale. | Keep artifacts retained, ask incident authority to narrow or release scope, and record the decision. |
| `backup_restore_drill_check.py` reports a missing evidence command. | A runbook or validation matrix no longer records the local test target. | Restore the documented command only after confirming the crate test still exists, or record the missing target as a planned gap. |

## References

- `AGENTS.md`
- `.agents/instructions/TRANSACTION_RECOVERY_STANDARD.md`
- `.agents/instructions/STORAGE_INVARIANTS_STANDARD.md`
- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`
- `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md`
- `documentations/specs/AuditLedger_v0.md`
- `documentations/specs/RecoveryReport_v0.md`
- `documentations/specs/SecurityAdmissionCanonicalOrder_v0.md`
- `documentations/testing/step-11-validation-matrix.md`
- `documentations/operations/runbooks/restore-pitr-drill.md`
