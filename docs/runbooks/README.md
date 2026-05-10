# Runbooks

> **Status:** Operations runbook index
> **Scope:** `docs/runbooks/`

## Purpose

This folder contains operator-facing procedures for incident response, recovery, and maintenance.
Runbooks describe action paths and evidence capture; they do not certify release readiness by
themselves.

Primary retained drill themes: Backup validation, restore drill, PITR, retention hold,
Planned or unplanned HA/DR failover, corruption suspicion, replica lag, WAL pressure, and forensic
startup.

## Runbook index

| Runbook | Purpose |
|---|---|
| `RUNBOOK_BACKUP_RESTORE.md` | Backup and restore response. |
| `RUNBOOK_CORRUPTION_SUSPICION.md` | Corruption suspicion triage. |
| `RUNBOOK_FORENSIC_START.md` | Forensic startup procedure. |
| `RUNBOOK_MAP_REFRESH_FAILURE.md` | Map refresh failure handling. |
| `RUNBOOK_NVME_PRESSURE.md` | NVMe pressure response. |
| `RUNBOOK_RECOVERY.md` | Recovery operations. |
| `RUNBOOK_REPLICA_LAG.md` | Replica lag response. |
| `RUNBOOK_SECURITY_INCIDENT.md` | Security incident response. |
| `RUNBOOK_SLOW_CLIENT_BACKPRESSURE.md` | Slow client and backpressure response. |
| `RUNBOOK_WAL_PRESSURE.md` | WAL pressure response. |

## Release evidence linkage

Operational release claims must cite both the runbook and the retained evidence
artifact produced by the drill or incident exercise. A runbook path alone is
navigation evidence, not release proof.

| Claim area | Required runbook linkage | Minimum retained evidence |
|---|---|---|
| Backup validation | `RUNBOOK_BACKUP_RESTORE.md` | Backup id, snapshot id, manifest identity, WAL range, audit entries, command transcript, and operator decision. |
| Restore drill | `RUNBOOK_BACKUP_RESTORE.md`, `RUNBOOK_RECOVERY.md` | Restore target, target LSN or time, `RestoreTrace`, `RecoveryReport`, recovered manifest, validation commands, and open-mode decision. |
| PITR | `RUNBOOK_BACKUP_RESTORE.md`, `RUNBOOK_RECOVERY.md` | PITR target, WAL archive range, replay stop reason, object/catalog validation, audit entries, and residual data-loss window. |
| Forensic startup | `RUNBOOK_FORENSIC_START.md`, `RUNBOOK_CORRUPTION_SUSPICION.md` | ForensicStart report, blocked application-connection proof, durable state summary, suspected corruption boundary, and escalation decision. |
| HA/DR failover | `RUNBOOK_REPLICA_LAG.md`, `RUNBOOK_RECOVERY.md` | Primary failure or partition scenario, quorum proof, fencing token proof, candidate recovery evidence, promotion transcript, manifest update, and replica repointing evidence. |
| WAL pressure | `RUNBOOK_WAL_PRESSURE.md` | WAL pressure trigger, retention decision, archive health, blocked truncation reason, and recovery-risk decision. |

## P00 release-operations rule

During P00, these runbooks establish the operational evidence vocabulary only.
They do not prove production backup, PITR, forensic startup, or HA/DR readiness
until retained drills are attached to a release evidence packet. Helper scripts
under `tools/testing/` may verify that local artifacts and documentation links
exist, but their output is local readiness evidence rather than production drill
proof.
