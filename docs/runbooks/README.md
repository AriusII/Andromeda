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
