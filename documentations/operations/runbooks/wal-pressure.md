# WAL Pressure Runbook

## Purpose

Use this runbook when WAL flush latency, WAL queue depth, WAL retention, or checkpoint pressure threatens commit latency or recovery safety.

## Scope

This runbook covers diagnosis, containment, and validation for WAL pressure on an Andromeda primary. It includes replica and backup retention checks when WAL truncation is being considered.

## Non-goals

This runbook does not authorize visible commit before durable WAL, truncate WAL outside retention policy, or move GPU, analytics, statistics, benchmark, or learned work into the commit, rollback, WAL, recovery, MVCC, catalog publication, or security-critical path.

## Prerequisites

- The operator has Administration Surface access, not Application Surface access, for operational controls.
- The operator can collect WAL flush latency, durable LSN, checkpoint, replica safe LSN, backup/PITR retention, and forensic retention evidence.
- Any pressure mitigation must preserve audit and recovery evidence.

## Status

| Area | Status | Evidence or limitation |
| --- | --- | --- |
| WAL-before-visible-commit doctrine | Implemented durable behavior where covered by WAL durability fences and tests; also a non-negotiable invariant. | `crates/andromeda-wal/src/write_ahead_log/durability_fence.rs`; `.agents/instructions/TRANSACTION_RECOVERY_STANDARD.md`. |
| WAL shipping validation | Implemented durable behavior for typed source/target identity, contiguous LSN chains, and replica ACK safe LSN tracking. | `crates/andromeda-storage/src/write_ahead_log/shipping.rs` and module tests. |
| WAL retention gates | Contract preview with some integration tests for GC boundaries. | `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`; `crates/andromeda-storage/tests/wal_gc_four_boundaries_integration.rs`. |
| Operator commands for pressure control | Contract preview unless a command is explicitly proven in code. | This runbook names actions, not stable CLI syntax. |

## Findings Ordered By Severity

| Severity | Finding | Exact invariant or contract affected | Concrete remediation |
| --- | --- | --- | --- |
| Critical | WAL pressure must never change the rule that commit visibility waits for durable WAL. | Invariant 3: Do not make a commit visible before durable WAL. Transaction and Recovery Standard: commit visible equals WAL durable. | Refuse or throttle new non-critical work before weakening durability. Keep commit acknowledgment gated by durable LSN evidence. |
| Critical | WAL truncation during pressure can break recovery, replica catch-up, PITR, or forensic investigation. | WAL retention contract: truncate only after verified snapshot, replica, backup/PITR, forensic retention, and MVCC pin gates. | Do not truncate until every retention boundary is verified. If a boundary is unknown, classify it as blocking. |
| High | Analytics, GPU, benchmark, statistics, and maintenance work can worsen WAL pressure while providing no C5 truth. | Invariants 4 and 5: RAM/temp/GPU/benchmark output is not truth; GPU is not in critical paths. | Suspend non-critical analytics, GPU batches, benchmark jobs, and statistics builds before throttling critical recovery or audit work. |
| Medium | Backpressure without traces makes later incident review unreliable. | AuditLedger v0: recovery, HA/DR, backup, restore, and forensic decisions preserve evidence. | Record RequestId, SessionId, policy decision, durable LSN, queue depth, and remediation action. |

## Procedure

1. Declare the incident.
   - Classify as WAL pressure.
   - Record current primary node, membership epoch, durable LSN, last visible commit LSN, queue depth, flush latency, checkpoint position, and open retention pins.
   - Mark this step as dry-run/verify-only until an operator action changes workload admission.

2. Protect commit durability.
   - Confirm that visible commit remains gated by durable WAL.
   - If the system cannot prove durable WAL, stop accepting new write work through the affected admission path.
   - Do not open a bypass through the Application Surface.

3. Reduce non-critical pressure.
   - Pause analytics, GPU batches, benchmark runs, statistics builds, scrubs, and low-priority maintenance.
   - Reduce or defer non-critical RPC workloads according to policy.
   - Preserve audit evidence for each throttle decision.

4. Check checkpoint and retention gates.
   - Verify the latest valid cold snapshot.
   - Verify replica safe LSNs for required replicas.
   - Verify backup/PITR retention LSN.
   - Verify forensic retention LSN.
   - Verify MVCC reader/version pins.
   - Treat any missing evidence as a planned gap or blocking unknown.

5. Decide whether to tune batching.
   - If policy permits, increase group commit batching within bounded latency limits.
   - Do not make batching adaptive unless it is observable, bounded, versioned, explainable, and disableable.

6. Recover to normal service.
   - Resume suspended work in priority order only after WAL latency and retention pressure return below policy thresholds.
   - Produce or attach incident evidence for review.

## Validation

Required before merge for runtime changes related to this runbook:

- WAL record codec tests.
- Gapless LSN tests.
- Checksum and truncation tests.
- Flush-boundary tests proving visible commit waits for durable WAL.
- Crash scenarios for before flush, after flush before dirty page, and after ack before dirty page flush.
- Retention tests covering snapshot, replica, backup/PITR, forensic, and MVCC pins.

Suggested existing gates to run when code changes affect WAL pressure behavior:

```powershell
cargo test -p andromeda-wal
cargo test -p andromeda-storage wal_gc_four_boundaries_integration
cargo test -p andromeda-storage write_ahead_log::shipping
```

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Flush latency rises while CPU analytics are active. | Non-critical work is competing with WAL. | Suspend analytics/statistics/benchmark work and preserve the decision trace. |
| WAL cannot be truncated. | Replica, PITR, forensic, or MVCC retention boundary still needs the range. | Keep WAL, increase storage headroom, or resync the blocking dependency. |
| Commit latency improves only after unsafe durability changes. | The mitigation violated C5 doctrine. | Revert the policy change through the owning runtime and open a release-blocking incident. |

## References

- `AGENTS.md`
- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`
- `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md`
- `.agents/instructions/TRANSACTION_RECOVERY_STANDARD.md`
- `.agents/instructions/QUALITY_GATES.md`
- `crates/andromeda-storage/src/write_ahead_log/shipping.rs`
