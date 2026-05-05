# TROUBLESHOOTING — Recovery and Administration Runbook

**Version:** 1.0.0  
**Updated:** Q1 2026  
**Language:** American English  
**Style:** Microsoft Documentation  

---

## Overview

This document provides systematic diagnosis and recovery procedures for common failure modes in Andromeda clusters. It covers:

- Crash recovery and data integrity verification
- Replication lag and split-brain scenarios
- Quorum loss and fencing challenges
- Catalog and manifest mismatches
- Performance degradation and audit tracing
- Backup, restore, and point-in-time recovery

All procedures assume the HA/DR runtime is operational. For local vertical prototype testing, see README.md.

---

## Systematic Troubleshooting Approach

### Step 1: Assess Cluster Health

**Command:**

```bash
andromeda-cli hadr status --json
```

**Interpretation Matrix:**

| Scenario | Status Indicators | Severity | Action |
|----------|------|----------|--------|
| **Normal** | Primary active, replicas Alive, lag = 0 bytes | ✓ | Proceed to Step 2 if noticing issues. |
| **Replica Lag** | Replicas Behind, lag_bytes > 0 | ⚠ | Check replication flow; see **"Replication Lag"** section. |
| **Replica Down** | Replicas Failed, no received_lsn | ⚠ | Check node connectivity; see **"Replica Node Failure"** section. |
| **Primary Down** | current_primary = null | 🔴 | Immediate failover required; see **"Primary Failure"** section. |
| **LSN Divergence** | durable_lsn ≠ committed_lsn | 🔴 | Critical: Log corruption; see **"WAL Corruption"** section. |

### Step 2: Inspect Log State

**Command:**

```bash
andromeda-cli recovery-inspect <wal-file-path>
```

**Outputs:**

```text
WAL File: /var/lib/andromeda/wal.segment
Durable Prefix: 1048576 bytes
Replay LSNs: [0, 1048576]
Ignored Transactions: [none]
Forensic Boundary: Safe (all records readable)
Last Commit LSN: 1048576
Highest Observed Epoch: 42
```

**Interpretation:**

| Field | Normal | Warning | Action |
|-------|--------|---------|--------|
| **Durable Prefix** | > 0 | = 0 | WAL is empty; cold start or initialization issue. |
| **Forensic Boundary** | Safe | Partial | Some records unreadable; recovery may lose recent transactions. |
| **Ignored Transactions** | empty | non-empty | Some transactions were skipped; possible corruption or intentional rollback. |
| **Replay LSNs** | Sequential | Gaps | LSN gaps detected; log may be corrupted. See **"LSN Gap Detection"**. |

---

## Failure Scenarios and Recovery

### WAL Corruption

**Symptoms:**

- `recovery-inspect` reports `Forensic Boundary: Partial`
- Replay fails with `InvalidWalFormat` or `LsnGapDetected`
- `durable_lsn != committed_lsn` after crash recovery

**Root Causes:**

1. Disk I/O failure during WAL write
2. Unclean shutdown (power loss, hard crash)
3. Storage backend corruption (RAID failure, bit flip)
4. Concurrent writes to WAL (synchronization bug)

**Diagnosis Steps:**

1. **Verify WAL integrity:**

   ```bash
   andromeda-cli recovery-inspect /var/lib/andromeda/wal.segment --detail
   ```

2. **Check for partial writes:**

   ```bash
   # Last record size should match header
   hexdump -C /var/lib/andromeda/wal.segment | tail -20
   ```

3. **Compare with backup:**

   ```bash
   andromeda-cli recovery-inspect /backup/wal.segment.safe --detail
   ```

**Recovery Options:**

#### Option A: Safe Recovery (Recommended)

**Impact:** May lose uncommitted transactions from the partial record onward.

**Steps:**

1. **Stop all replicas.**

   ```bash
   systemctl stop andromeda-replica-*
   ```

2. **Identify the safe cutoff:**

   ```bash
   andromeda-cli recovery-inspect /var/lib/andromeda/wal.segment --safe-cutoff
   ```

   Output: `Safe LSN: 1047552 (last complete record)`

3. **Truncate WAL at safe LSN:**

   ```bash
   andromeda-cli wal-truncate /var/lib/andromeda/wal.segment --lsn 1047552
   ```

4. **Restart primary:**

   ```bash
   systemctl start andromeda-primary
   ```

5. **Verify recovery:**

   ```bash
   andromeda-cli hadr status
   # Confirm durable_lsn == 1047552
   ```

6. **Re-sync replicas:**

   ```bash
   for replica_id in 2 3; do
     systemctl restart andromeda-replica-$replica_id
   done
   ```

7. **Monitor lag:**

   ```bash
   watch -n 1 'andromeda-cli hadr status | grep lag'
   ```

#### Option B: Point-in-Time Recovery (From Backup)

**Impact:** Restores to last clean backup; may lose transactions since backup.

**Steps:**

1. **Locate latest backup:**

   ```bash
   ls -lt /backup/andromeda-snapshots/ | head -5
   # Example: /backup/andromeda-snapshots/2026-01-15-14-00-00/
   ```

2. **Verify backup integrity:**

   ```bash
   andromeda-cli backup-verify /backup/andromeda-snapshots/2026-01-15-14-00-00/manifest.json
   ```

3. **Stop all nodes:**

   ```bash
   systemctl stop andromeda-primary
   systemctl stop andromeda-replica-*
   ```

4. **Restore from backup:**

   ```bash
   andromeda-cli restore \
     --backup-path /backup/andromeda-snapshots/2026-01-15-14-00-00/ \
     --data-dir /var/lib/andromeda/
   ```

5. **Restart primary:**

   ```bash
   systemctl start andromeda-primary
   ```

6. **Verify restored state:**

   ```bash
   andromeda-cli hadr status
   ```

**Audit Event:** Emit DEC-033 audit record with backup ID, restoration timestamp, and transactions lost count.

---

### Replication Lag

**Symptoms:**

- `hadr status` shows replicas with `lag_bytes > 0`
- Replicas marked as "Behind" or "Failed"
- Replication stream appears stuck

**Root Causes:**

1. Network latency or packet loss
2. Replica disk I/O bottleneck
3. Primary WAL shipping thread blocked
4. Replica recovery process slow

**Diagnosis Steps:**

1. **Measure lag in real-time:**

   ```bash
   andromeda-cli hadr status --json | jq '.replicas[] | {replica_id, lag_bytes}'
   ```

2. **Check replica receiver thread:**

   ```bash
   andromeda-cli hadr node status <replica-id> --json
   # Look for received_lsn vs. shipped_lsn
   ```

3. **Monitor shipping rate (bytes/sec):**

   ```bash
   # Sample lag every 5 seconds
   for i in {1..10}; do
     lag=$(andromeda-cli hadr status --json | jq '.replicas[0].lag_bytes')
     echo "Lag: $lag bytes"
     sleep 5
   done
   ```

4. **Check network connectivity:**

   ```bash
   # Test replication channel (QUIC on default port 5433)
   nc -zv replica-node-2 5433
   ```

**Recovery Steps:**

#### If lag is growing (shipping stuck):

1. **Stop receiving thread on replica:**

   ```bash
   andromeda-cli hadr node demote <replica-id> --dry-run
   ```

2. **Restart replication stream:**

   ```bash
   systemctl restart andromeda-replica-<replica-id>
   ```

3. **Monitor catch-up:**

   ```bash
   watch -n 1 'andromeda-cli hadr status | grep -A 5 "Replica <replica-id>"'
   ```

#### If lag is stable but non-zero (normal):

- **No action required.** Lag converges as primary continues shipping.
- Monitor that lag does not exceed acceptable threshold (e.g., 10 MB).

---

### Replica Node Failure

**Symptoms:**

- `hadr status` shows replica with `health_state: Failed`
- Replica not responding to network probes
- `lag_bytes` continuously increasing or stuck

**Root Causes:**

1. Replica process crashed or hung
2. Disk full or I/O error on replica
3. Network partition (split-brain risk)
4. Out-of-memory condition

**Diagnosis Steps:**

1. **Check replica process:**

   ```bash
   systemctl status andromeda-replica-<replica-id>
   journalctl -u andromeda-replica-<replica-id> -n 50 --no-pager
   ```

2. **Check disk space:**

   ```bash
   df -h /var/lib/andromeda/
   du -sh /var/lib/andromeda/heap
   du -sh /var/lib/andromeda/wal
   ```

3. **Check memory:**

   ```bash
   free -h
   ps aux | grep andromeda
   ```

4. **Test network reachability:**

   ```bash
   ping replica-node-2
   nc -zv replica-node-2 5433
   ```

**Recovery Steps:**

#### If process is down:

1. **Restart replica:**

   ```bash
   systemctl start andromeda-replica-<replica-id>
   ```

2. **Monitor recovery:**

   ```bash
   journalctl -u andromeda-replica-<replica-id> -f
   ```

3. **Verify catch-up:**

   ```bash
   andromeda-cli hadr status | grep "Replica <replica-id>"
   ```

#### If disk is full:

1. **Free space by archiving old WAL segments:**

   ```bash
   andromeda-cli wal-archive --days-retention 7
   ```

2. **Compact heap:**

   ```bash
   andromeda-cli heap-compact
   ```

3. **Monitor disk:**

   ```bash
   df -h /var/lib/andromeda/
   ```

#### If network partitioned (suspected split-brain):

1. **Check fencing status:**

   ```bash
   andromeda-cli hadr quorum status
   ```

2. **Verify epoch alignment:**

   ```bash
   andromeda-cli hadr status --json | jq '.epoch'
   ```

3. **If epochs differ, manual intervention required:**

   - Contact system administrators.
   - Isolate the failed replica (bring it offline).
   - Verify primary still has quorum.
   - Perform demotion and failover if needed.

---

### Primary Failure

**Symptoms:**

- `hadr status` returns "Primary not reachable"
- `current_primary: null` in status JSON
- Cluster is read-only; no new transactions accepted

**Root Causes:**

1. Primary node crashed or hung
2. Primary disk failure
3. Primary network partition
4. Out-of-memory or unhandled panic

**Diagnosis Steps:**

1. **Check primary process:**

   ```bash
   ssh primary-node-1
   systemctl status andromeda-primary
   journalctl -u andromeda-primary -n 100 --no-pager
   ```

2. **Check primary reachability:**

   ```bash
   nc -zv primary-node-1 5433
   ping primary-node-1
   ```

3. **Verify quorum remains intact:**

   ```bash
   andromeda-cli hadr quorum status
   # Should show sufficient members to form quorum without primary
   ```

**Recovery Steps:**

#### Option A: Restart Primary (if reachable)

1. **Restart primary process:**

   ```bash
   ssh primary-node-1
   systemctl restart andromeda-primary
   ```

2. **Monitor startup:**

   ```bash
   journalctl -u andromeda-primary -f
   ```

3. **Verify primary is accepting connections:**

   ```bash
   andromeda-cli hadr status
   ```

#### Option B: Failover to Replica (if primary unrecoverable)

1. **Select promotion candidate (lowest lag):**

   ```bash
   andromeda-cli hadr status --json | jq '.replicas | sort_by(.lag_bytes) | .[0].replica_id'
   ```

2. **Promote replica to primary:**

   ```bash
   andromeda-cli hadr promote <best-replica-id>
   ```

3. **Monitor new primary:**

   ```bash
   andromeda-cli hadr status
   # Verify cluster_role: Primary, epoch incremented
   ```

4. **Re-enroll failed primary as replica:**

   ```bash
   # Once primary is recovered:
   systemctl restart andromeda-primary
   # It will rejoin as replica automatically
   ```

**Audit Event:** Emit DEC-033 audit record with failover reason, old and new primary IDs, and epoch transition.

---

### Quorum Loss

**Symptoms:**

- `hadr quorum status` shows fewer than quorum_size members alive
- `andromeda-cli hadr promote` fails with "PromotionFencingConflict"
- Cluster is frozen; no mutations allowed

**Root Causes:**

1. Multiple replica failures simultaneously
2. Network partition dividing cluster
3. Fencing token expired or overwritten

**Prevention:**

- Deploy odd-numbered quorum (3, 5, 7) for tolerance of (1, 2, 3) failures.
- Monitor replica health continuously.
- Implement automated failover for single-replica failures.

**Recovery Steps:**

1. **Assess damage:**

   ```bash
   andromeda-cli hadr quorum status --json
   ```

2. **Bring offline replicas back online:**

   ```bash
   for node_id in <failed-node-ids>; do
     ssh replica-node-$node_id
     systemctl restart andromeda-replica-$node_id
   done
   ```

3. **Wait for re-sync and quorum restoration:**

   ```bash
   watch -n 2 'andromeda-cli hadr quorum status'
   # Wait for total_members to reach quorum_size
   ```

**If recovery is impossible:**

- **Contact system administrators** for manual intervention.
- Document the incident and root cause.
- Plan for infrastructure upgrade or quorum size adjustment.

---

### Manifest Mismatch

**Symptoms:**

- `recovery-inspect` or manifest validation reports mismatch
- Backup/restore operations fail
- Catalog queries return stale metadata

**Root Causes:**

1. Manifest file corrupted or not synced to disk
2. Catalog mutation happened without manifest update
3. Backup manifest predates recent catalog changes

**Diagnosis Steps:**

1. **Verify manifest integrity:**

   ```bash
   andromeda-cli backup-verify /var/lib/andromeda/manifest.json
   ```

2. **Compare manifest with runtime catalog:**

   ```bash
   andromeda-cli catalog-dump --format json > /tmp/catalog.json
   # Manually compare with manifest procedures and tables
   ```

3. **Check manifest last-modified time:**

   ```bash
   stat /var/lib/andromeda/manifest.json
   ```

**Recovery Steps:**

1. **Regenerate manifest from current catalog:**

   ```bash
   andromeda-cli manifest-rebuild --output /var/lib/andromeda/manifest.json
   ```

2. **Verify new manifest:**

   ```bash
   andromeda-cli backup-verify /var/lib/andromeda/manifest.json
   ```

3. **Restart services to reload manifest:**

   ```bash
   systemctl restart andromeda-primary
   systemctl restart andromeda-replica-*
   ```

4. **Confirm consistency:**

   ```bash
   andromeda-cli hadr status
   ```

---

## Audit and Monitoring

### Audit Ledger Inspection

All critical operations emit audit events to the durable audit ledger (DEC-033). Inspect with:

```bash
andromeda-cli audit query --since "2 hours ago" --event-type promotion,failover,node-register
```

**Example Output:**

```
2026-01-15T14:30:45Z | promotion        | replica_id=2 | success | epoch=43 | operator=alice@company.com
2026-01-15T14:25:30Z | node-register    | node_id=4    | dry-run | message=contract_validated
2026-01-15T14:20:00Z | replication-lag  | lag_bytes=1024 | warning
```

### Common Queries

**Find recent failovers:**

```bash
andromeda-cli audit query --event-type failover --since "24 hours ago"
```

**Find failed promotion attempts:**

```bash
andromeda-cli audit query --event-type promotion --outcome failed --since "7 days ago"
```

**Find nodes that have been deregistered:**

```bash
andromeda-cli audit query --event-type node-deregister
```

---

## Performance Troubleshooting

### Slow Query Execution

**Symptoms:**

- Procedure invocations exceed latency budget (see BENCHMARK.md)
- Throughput drops below expected
- End-to-end latency high but CPU/disk idle

**Diagnosis:**

1. **Run benchmark to measure baseline:**

   ```bash
   andromeda-cli benchmark run inventory-reserve-stock --diagnostic-json
   ```

2. **Compare against budget:**

   ```json
   {
     "budget": {
       "latency_p99_threshold_ms": 50,
       "latency_status": "FAIL" // actual p99 = 120 ms
     }
   }
   ```

3. **Identify bottleneck (plan cache, optimizer, etc.):**

   ```bash
   andromeda-cli plan-cache status --json
   ```

**Recovery:**

1. **Clear plan cache:**

   ```bash
   andromeda-cli plan-cache clear
   ```

2. **Re-analyze statistics:**

   ```bash
   andromeda-cli catalog-analyze-stats
   ```

3. **Re-run benchmark:**

   ```bash
   andromeda-cli benchmark run inventory-reserve-stock --diagnostic-json
   ```

---

## Glossary

| Term | Meaning |
|------|---------|
| **Durable LSN** | Highest LSN flushed to persistent storage at primary. |
| **Safe LSN** | Highest LSN durably persisted at a replica. |
| **Replica Lag** | Difference in bytes between primary durable_lsn and replica received_lsn. |
| **Quorum** | Minimum node count required for decision authority. |
| **Failover** | Unplanned transition to a new primary due to primary failure. |
| **Fencing Token** | Epoch-bound proof authorizing a primary. |
| **Audit Ledger** | Durable record of critical operations (DEC-033). |
| **Manifest** | Persistent snapshot of catalog metadata and backup metadata. |
| **Forensic Boundary** | Last readable record in WAL; beyond boundary, records are unreadable. |

---

## Related Documentation

- **CLI.md:** Command reference for diagnostics
- **DEC-020:** Quorum Runtime (fencing policy)
- **DEC-024:** Promotion and Failover Boundary
- **DEC-033:** Durable Audit Ledger
- **README.md:** Local testing procedures

---

## Support Escalation

If none of the above procedures resolve the issue:

1. **Collect diagnostic bundle:**

   ```bash
   andromeda-cli diagnostic-bundle --output /tmp/bundle.tar.gz
   # Includes: status, recovery-inspect, audit logs, manifests, system info
   ```

2. **Contact support with:**

   - Diagnostic bundle
   - Steps already taken
   - Timeline of observed symptoms
   - Audit ledger export (last 24 hours)

3. **Expected response time:**

   - P1 (data loss): < 1 hour
   - P2 (cluster degraded): < 4 hours
   - P3 (warning/informational): < 1 business day

