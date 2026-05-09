---
name: hadr-backup-forensic
description: "Use for HA/DR, quorum, fencing, backup, restore, PITR and forensic startup."
category: andromeda-operations
---

# hadr-backup-forensic

## When to use
Availability, backup, restore or incident response is involved.

## Purpose
Use for HA/DR, quorum, fencing, backup, restore, PITR and forensic startup.

## Process
- Single Primary + Replicas in V0; no multi-primary.
- Use quorum and fencing to prevent split-brain.
- Distinguish backup from replica.
- Test restore/PITR regularly.
- ForensicStart blocks application connections and emits report.

## Expected output
- HA/DR or backup runbook.

## Guardrails
- No self-promotion without quorum.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
