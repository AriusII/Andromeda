# Replica Lag Runbook

## Purpose

Use this runbook when a replica falls behind the primary, RPO/RTO targets are at risk, WAL retention is growing, or promotion eligibility is being considered.

## Scope

This runbook covers single-primary HA/DR with replicas, quorum, fencing, WAL shipping, RPO/RTO review, and promotion safety. It does not cover multi-primary operation.

## Non-goals

This runbook does not authorize automatic promotion without majority quorum, fencing evidence, membership epoch advancement, LSN ranking, and recovery to a consistent LSN. It does not treat a replica as a backup.

## Prerequisites

- Current primary identity, membership epoch, quorum membership, and node roles are known.
- Last durable LSN, last applied LSN, replica safe LSN, and replication lag are available for each replica.
- RPO and RTO targets are declared. A configuration without explicit RPO/RTO is incomplete.
- Fencing authority is available before promotion is attempted.

## Status

| Area | Status | Evidence or limitation |
| --- | --- | --- |
| V0 topology | Contract preview and doctrine: Single Primary plus Replicas; no multi-primary in V0. | `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md`. |
| WAL shipping validation | Implemented durable behavior for typed primary-to-replica validation, contiguous LSN chain checks, and ACK safe LSN tracking. | `crates/andromeda-storage/src/write_ahead_log/shipping.rs`. |
| Promotion and membership gates | Partially implemented contract behavior with tests for membership and promotion boundaries. | `crates/andromeda-storage/tests/hadr_membership_store_contract.rs`; `crates/andromeda-storage/tests/promotion_boundary_contract.rs`; CLI HADR tests. |
| Full cluster simulation | Planned gap. | `documentations/ROADMAP_IMPLEMENTATION_2026.md` states full backup/restore drills and cluster simulation remain future work. |

## Findings Ordered By Severity

| Severity | Finding | Exact invariant or contract affected | Concrete remediation |
| --- | --- | --- | --- |
| Critical | A lagging or divergent replica must not be promoted. | HADR V0 quorum and fencing contract; majority quorum; membership epoch; LSN ranking; absence of forbidden divergence. | Mark the replica ineligible until it catches up or is resynced from a valid snapshot and WAL range. |
| Critical | Failing to fence a suspect primary can create split brain. | HADR fencing contract; RISK-010 HADR split-brain and missing fencing. | Require fencing evidence before promotion and preserve HA/DR decision audit records. |
| High | WAL needed for replica catch-up must not be truncated. | WAL retention contract: replicas must have applied or be scheduled for resync before truncation. | Hold WAL through the replica safe LSN or choose snapshot resync before reclaiming WAL. |
| High | RPO/RTO without measured lag is only an assertion. | HA/DR replication contract: RPO/RTO must be explicit. Invariant 9: optimization and adaptive behavior must be observable. | Record lag bytes, lag time, safe LSN, apply rate, and estimated catch-up time. |
| Medium | Treating a replica as a backup fails to protect against logical deletion or corruption. | Backup is not HA/DR doctrine. | Use restore/PITR runbook for data loss, corruption, ransomware, or bad migration scenarios. |

## Procedure

1. Classify the lag.
   - Record primary node, replica node, membership epoch, last durable LSN, last applied LSN, safe LSN, lag bytes, lag time, and apply rate.
   - Compare observed lag against RPO and RTO targets.

2. Protect WAL retention.
   - Hold WAL required by the lagging replica unless a snapshot resync decision is made.
   - Verify backup/PITR and forensic retention are not being weakened to relieve lag pressure.

3. Decide whether to degrade or resync.
   - For asynchronous remote replicas, degrade read or promotion eligibility when lag violates policy.
   - For excessive lag, initiate snapshot resync as a contract preview or dry-run unless a proven runtime path exists.
   - Do not promote a replica that lacks the required durable LSN, catalog/manifest compatibility, or health evidence.

4. Check quorum and fencing before any promotion.
   - Verify majority quorum or configured quorum rule.
   - Verify the suspect primary is fenced.
   - Verify the candidate has the highest eligible durable LSN.
   - Verify membership epoch advancement.
   - Verify recovery to a consistent LSN.
   - Emit HA/DR decision evidence.

5. Restore normal replication.
   - Resume WAL shipping.
   - Confirm ACK safe LSN advances.
   - Confirm RPO/RTO measurements return to policy.
   - Clear degraded status only after evidence is captured.

## Validation

Required before merge for runtime changes related to this runbook:

- WAL shipping tests for source role, target role, contiguous LSN chain, first LSN expectation, previous LSN mismatch, duplicate LSN, empty batch, and overflow.
- Replica ACK safe LSN tests and WAL retention tests.
- Quorum tests for majority, missing vote, stale vote, and membership epoch mismatch.
- Fencing tests for stale primary, failed fence, double-promotion attempt, and partitioned cluster.
- Promotion tests proving the highest eligible durable LSN wins and lagging replicas are rejected.
- Crash/recovery tests for promotion during WAL shipping and manifest publication.

Suggested commands when related code changes exist:

```powershell
cargo test -p andromeda-storage hadr
cargo test -p andromeda-storage promotion
cargo test -p andromeda-storage write_ahead_log::shipping
cargo test -p andromeda-cli cli_admin_commands::hadr
```

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Replica lag grows while WAL shipping succeeds. | Apply path is slower than receive path. | Degrade replica role, increase retention headroom, or schedule snapshot resync. |
| Replica cannot catch up because WAL was reclaimed. | Retention gate failed. | Use snapshot resync and open a retention policy defect. |
| Promotion is requested during lag. | Availability pressure is overriding safety. | Require quorum, fencing, LSN ranking, and recovery evidence before promotion. |

## References

- `AGENTS.md`
- `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md`
- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`
- `.agents/instructions/TRANSACTION_RECOVERY_STANDARD.md`
- `.agents/registries/risk-register.yaml`
- `crates/andromeda-storage/src/write_ahead_log/shipping.rs`
