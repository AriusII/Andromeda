# HA/DR Runbook

## Purpose

Use this runbook for planned failover, unplanned primary suspicion, replica
promotion, quorum validation, fencing, and replica repointing.

## Prerequisites

- Current primary, replica set, quorum members, membership epoch, and fencing
  authority are known.
- Candidate replicas report durable LSN, applied LSN, safe LSN, health, lag,
  and divergence status from durable or auditable evidence.
- WAL retention is protected for promotion, lagging replicas, backup/PITR, and
  forensic hold.
- HA/DR controls are available only through Administration or HA/DR surfaces.

## Procedure

1. Classify the event as planned failover, suspected primary failure, network
   partition, or rehearsal.
2. Pause or reject write traffic according to policy and preserve admission
   evidence for any rejected application requests.
3. Freeze WAL reclamation for the affected database and epoch.
4. Capture cluster state, quorum state, candidate LSNs, WAL chain evidence,
   audit ids, and current manifest identity.
5. Verify primary suspicion. Do not promote on client reachability alone.
6. Rank candidates by membership eligibility, health, non-divergent WAL chain,
   durable LSN, and RPO/RTO fit.
7. Verify quorum for the current membership epoch.
8. Fence the old primary. Abort promotion when fencing is missing, stale, or
   ambiguous.
9. Recover the candidate to a consistent LSN and produce `RecoveryReport`
   evidence.
10. Publish the new primary and membership epoch only after quorum, fencing,
    LSN, and recovery gates pass.
11. Repoint replicas, reject stale or divergent followers, and keep WAL retained
    until catch-up and backup/PITR gates are safe.
12. Reopen application traffic in the narrowest approved mode.

## Validation Commands

```powershell
cargo test -p andromeda-hadr --test hadr_promotion_runtime_contract --locked -- --nocapture
cargo test -p andromeda-hadr --test hadr_membership_store_contract --locked -- --nocapture
cargo test -p andromeda-hadr --test quorum_membership_contract --locked -- --nocapture
cargo test -p andromeda-hadr --test wal_shipping_reclaimability_contract --locked -- --nocapture
cargo test -p andromeda-rpc-protocol --test surface_separation_contract --locked -- --nocapture
cargo test -p andromeda-audit --test hadr_backup_audit_contract --locked -- --nocapture
```

## Escalate When

- No candidate can prove a safe durable LSN.
- Quorum or fencing cannot be proven.
- WAL divergence appears.
- Audit or recovery evidence is missing.
- Application traffic can invoke promotion or cluster controls.
