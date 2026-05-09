# Runbooks

This directory is the `/docs` home for concise operational response guides.
Runbooks describe containment and evidence expectations; they do not define new
runtime behavior or stable CLI contracts.

## Runbook Index

| Situation | Runbook | Primary risk |
| --- | --- | --- |
| Backup validation, restore drill, PITR, retention hold | `backup-restore.md` | Untested backup, incomplete WAL replay, or unsafe opening. |
| Planned or unplanned HA/DR failover | `hadr.md` | Split brain, stale promotion, missing quorum, or missing fencing. |
| Corruption suspicion or unsafe startup evidence | `corruption.md` | Silent corruption or replay past a corruption boundary. |
| Slow query, slow client, benchmark triage, resource pressure | `performance.md` | Misreading advisory diagnostics as durable truth. |
| WAL queue growth, flush latency, retention pressure | `wal-pressure.md` | Visible commit before durable WAL or unsafe WAL truncation. |
| Replica lag, RPO/RTO risk, promotion eligibility | `replica-lag.md` | Stale promotion, broken retention, or split brain. |

## Operating Rules

- Preserve WAL, audit, manifest, backup, restore, and cluster evidence before
  destructive remediation.
- Keep Administration and HA/DR controls off the Application Surface.
- Treat command examples as evidence shapes unless the runtime owner has proven
  the command in source and tests.
- Record residual risk when a step is a dry run, contract preview, or planned
  gap.
