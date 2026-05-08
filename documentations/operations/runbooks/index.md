# Andromeda Operations Runbook Index

## Purpose

This index routes operators to the minimum runbook for common HA/DR, WAL, backup, restore, and forensic incidents.

## Scope

The runbooks in this directory cover operational response for:

- WAL pressure.
- Slow clients and stream backpressure.
- Replica lag, quorum impact, and promotion safety.
- Corruption suspicion and ForensicStart containment.
- Restore and point-in-time recovery drills.
- HA/DR failover, quorum, fencing, promotion, and replica repointing.
- Forensic startup, recovery reports, and application traffic blocking.
- Backup retention, WAL archive retention, and forensic hold.

## Non-goals

These runbooks do not define new runtime behavior, CLI syntax, RPC endpoints, or storage formats. They do not claim that a full backup/restore drill or cluster simulation is implemented unless source evidence proves it.

## Prerequisites

- Operators must understand the Andromeda surfaces: Application Surface, Administration Surface, and HA/DR Cluster Surface.
- Administration, backup, restore, failover, quorum, fencing, WAL shipping, and forensic startup must not be exposed through the Application Surface.
- Any command examples in linked runbooks that are marked as contract preview or dry-run/verify-only must not be treated as production automation.

## Status Legend

| Status | Meaning |
| --- | --- |
| Implemented durable behavior | Source code or tests currently provide durable behavior or validation evidence. |
| Contract preview | Architecture, spec, or typed boundary exists, but full runtime orchestration is not proven here. |
| Dry-run/verify-only | The step is safe for inspection or rehearsal and must not publish state or open production traffic. |
| Planned gap | The repository documents the need, but implementation, integration, or deterministic drill evidence is still pending. |

## Runbooks

| Incident or drill | Runbook | Primary risk |
| --- | --- | --- |
| WAL flush latency, WAL queue growth, retention pressure | `wal-pressure.md` | Visible commit before durable WAL or WAL truncation that breaks recovery. |
| Slow client, stream congestion, send buffer pressure | `slow-client.md` | Backpressure failure that stalls critical paths or drops audit/recovery evidence. |
| Replica lag, promotion question, RPO/RTO degradation | `replica-lag.md` | Split brain, stale promotion, or broken WAL retention. |
| Planned or unplanned HA/DR failover | `hadr-failover.md` | Split brain, stale promotion, unfenced old primary, or unsafe recovery target. |
| Corruption suspicion, checksum mismatch, manifest inconsistency | `corruption-suspicion.md` | Silent corruption or replay past a corruption boundary. |
| Startup anomaly, unknown format, corruption boundary, forensic investigation | `forensic-startup.md` | Mutating inspected truth or reopening Application Surface traffic before evidence passes. |
| Restore, PITR, backup validation, disaster recovery rehearsal | `restore-pitr-drill.md` | Untested backup, incomplete WAL replay, or unsafe production opening. |
| Backup retention, WAL archive cleanup, retention hold, forensic hold | `backup-retention.md` | Deleting artifacts needed for restore, PITR, replica catch-up, audit, or incident evidence. |

## Local Drill Checks

Use these local checks before claiming that the repository has executable backup/restore or HA/DR drill evidence. They inspect local files, runbooks, test targets, and documented evidence commands. They do not restore data, replay WAL, start cluster nodes, open sockets, publish cluster manifests, or fence a real primary.

| Check | Command | What it proves | What it does not prove |
| --- | --- | --- | --- |
| Backup, restore, and PITR drill readiness | `python tools/testing/backup_restore_drill_check.py` | Required local runbooks, test targets, and evidence commands are present. | A production restore can complete, meet RPO/RTO, or open safely. |
| Backup, restore, and PITR drill readiness as JSON | `python tools/testing/backup_restore_drill_check.py --json` | The same local evidence is available to CI or release reports as structured output. | Durable restore execution or exact LSN replay. |
| HA/DR cluster drill readiness | `python tools/testing/hadr_cluster_drill_check.py` | Required local runbooks, test targets, and HA/DR evidence commands are present. | A real cluster can fail over, fence a primary, or repoint replicas. |
| HA/DR cluster drill readiness as JSON | `python tools/testing/hadr_cluster_drill_check.py --json` | The same local evidence is available to CI or release reports as structured output. | Network partitions, real quorum, or production fencing behavior. |

## Findings Ordered By Severity

| Severity | Finding | Exact invariant or contract affected | Concrete remediation |
| --- | --- | --- | --- |
| Critical | Full backup/restore drills and cluster simulation remain planned gaps in the roadmap evidence. | Invariant 10: Do not accept mission-critical behavior without crash/recovery validation. Transaction and Recovery Standard: canonical state is the last valid cold snapshot plus durable WAL. | Treat every restore as unproven until a drill validates snapshot, WAL range, catalog/security invariants, and opening criteria. |
| Critical | Replica lag can become a promotion and retention hazard if quorum, fencing, and LSN ranking are not evaluated together. | HADR V0 Single Primary plus Replicas; majority quorum for promotion; membership epoch; LSN ranking; Invariant 8: do not expose HA/DR through Application Surface. | Block promotion for lagging or divergent replicas, preserve WAL retention, and require audit evidence for quorum and fencing decisions. |
| Critical | HA/DR failover without durable quorum, fencing, LSN, and recovery evidence can create split brain or publish stale truth. | HADR V0 Single Primary plus Replicas; Invariant 3; Invariant 8; Invariant 10. | Promote only after quorum, fencing, candidate LSN eligibility, RecoveryReport evidence, and cluster manifest publication gates pass. |
| Critical | Corruption suspicion must fail closed before application traffic resumes. | Invariant 10; RecoveryReport contract; AuditLedger v0 forensic and recovery decision families. | Enter SafeStart or ForensicStart, block application traffic, preserve evidence, and only reopen after explicit validation. |
| Critical | Backup retention cleanup can destroy the only restorable WAL or snapshot evidence if retention pins are incomplete. | Transaction and Recovery Standard: canonical state is the last valid cold snapshot plus durable WAL. | Preflight every deletion against backup, PITR, replica, MVCC, audit, compliance, and forensic-hold dependencies. |
| High | WAL pressure can tempt unsafe throttling or truncation decisions. | Invariant 3: Do not make a commit visible before durable WAL. WAL retention boundaries: snapshot, replicas, backups/PITR, forensic retention, MVCC pins. | Reduce non-critical load, stop analytics/statistics work first, and do not truncate WAL until all retention gates pass. |
| High | Slow clients can hide protocol and audit pressure behind normal-looking sessions. | ResultStream/backpressure contract preview; AuditLedger v0 admission and recovery evidence; Invariant 9 for bounded and observable adaptive behavior. | Apply bounded backpressure, enforce quotas, preserve RequestId/SessionId evidence, and close abusive sessions after policy thresholds. |

## Procedure

1. Classify the incident by the table in this index.
2. Open the matching runbook.
3. Record whether each action is implemented durable behavior, contract preview, dry-run/verify-only, or a planned gap.
4. Preserve audit, WAL, RecoveryReport, RestoreTrace, or ForensicReport evidence before destructive remediation.
5. Run the local drill check when the incident or rehearsal involves backup/restore/PITR or HA/DR failover.
6. Escalate to the owning storage/recovery, HA/DR, backup/restore, security, or observability owner when a runbook reaches a planned gap.

## Validation

For documentation-only changes to these runbooks, validate with:

```powershell
git diff -- documentations/operations/runbooks
rg -n "Application Surface|ad hoc SQL|gRPC|implemented" documentations/operations/runbooks
python tools/testing/backup_restore_drill_check.py
python tools/testing/backup_restore_drill_check.py --json
python tools/testing/hadr_cluster_drill_check.py
python tools/testing/hadr_cluster_drill_check.py --json
```

Before merging runtime work implied by these runbooks, use the strongest applicable gate:

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
cargo test --doc --workspace
```

Mission-critical WAL, storage, HA/DR, backup, restore, and forensic changes also require deterministic crash/recovery, PITR, corruption, quorum, and fencing validation.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| A runbook step appears to require an Application Surface call. | Surface separation drift. | Stop and route through Administration Surface or HA/DR Cluster Surface only. |
| A runbook step claims production readiness without a test name or source. | Documentation overclaim. | Reclassify as contract preview or planned gap. |
| A restore or failover procedure lacks audit evidence. | Missing critical decision trace. | Block readiness and require the owning implementation to emit durable evidence. |
| A local drill check fails on a missing path. | The expected runbook, test target, source module, or validation matrix path moved or was removed. | Confirm the owning artifact and update the runbook or checker only after source evidence is clear. |
| A local drill check passes but release evidence is still absent. | The check is read-only or simulation-only and does not run production recovery or cluster behavior. | Keep release readiness blocked until a recorded drill proves the required runtime behavior. |

## References

- `AGENTS.md`
- `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md`
- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`
- `documentations/ROADMAP_IMPLEMENTATION_2026.md`
- `documentations/specs/AuditLedger_v0.md`
- `.agents/instructions/TRANSACTION_RECOVERY_STANDARD.md`
- `.agents/instructions/QUALITY_GATES.md`
