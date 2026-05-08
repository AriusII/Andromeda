# Runbooks

This directory contains operational runbooks for Andromeda deployment, maintenance, and incident response.

## Purpose

Runbooks provide:
- Step-by-step operational procedures
- Incident response playbooks
- Troubleshooting guides
- Recovery procedures

## Contents

### Startup and Shutdown

- **startup-modes.md** - FastStart, SafeStart, ForensicStart procedures
- **shutdown-procedures.md** - Graceful shutdown, emergency shutdown
- **bootstrap-new-instance.md** - Initial deployment and configuration

### Backup and Recovery

- **backup-and-restore.md** - Backup policies, restore procedures, PITR workflow
- **disaster-recovery.md** - Full site recovery, failover, switchback
- **point-in-time-recovery.md** - PITR restore to specific timestamp

### High Availability and Disaster Recovery

- **hadr-failover.md** - Primary failure detection, replica promotion, quorum recovery
- **replication-procedures.md** - Replica startup, WAL shipping, catch-up synchronization
- **split-brain-prevention.md** - Fencing, quorum, epoch handling

### Maintenance

- **index-maintenance.md** - Index rebuild, defragmentation, ANALYZE statistics
- **wal-archival.md** - WAL rotation, archival retention, cleanup
- **coldstore-management.md** - Segment migration, archive policies

### Troubleshooting

- **performance-diagnosis.md** - Slow queries, lock contention, buffer pool issues
- **corruption-detection.md** - Corruption symptoms, forensic startup, remediation
- **replication-lag.md** - Detecting and resolving replication lag
- **consistency-checks.md** - Running consistency checks, repair procedures

### Security

- **user-and-privilege-management.md** - User creation, privilege grants, audit logging
- **tls-certificate-management.md** - Certificate rotation, renewal, troubleshooting
- **audit-trail-review.md** - Audit log queries, compliance reporting

## Format

Each runbook:
- Has clear prerequisites
- Lists estimated duration
- Provides step-by-step instructions with checkpoints
- Includes rollback procedures
- Includes validation/verification steps

## Cross-References

- Architecture (docs/architecture/) for design context
- Specifications (docs/specifications/) for detailed semantics
