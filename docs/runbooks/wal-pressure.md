# WAL Pressure Runbook

## Purpose

Use this runbook when WAL flush latency, WAL queue depth, checkpoint pressure,
or retention pressure threatens commit latency or recovery safety.

## Prerequisites

- Current durable LSN, visible commit LSN, WAL queue depth, flush latency,
  checkpoint position, replica safe LSNs, backup/PITR bounds, forensic holds,
  and MVCC pins are available or recorded as unknown.
- Operators can throttle workload through approved Administration controls.

## Procedure

1. Declare WAL pressure and record primary node, membership epoch, durable LSN,
   visible commit LSN, queue depth, flush latency, checkpoint position, and open
   retention pins.
2. Confirm visible commit remains gated by durable WAL.
3. If durable WAL cannot be proven, stop admitting new writes for the affected
   path until evidence is restored.
4. Reduce non-critical load first: analytics, statistics builds, benchmarks,
   scrubs, and low-priority maintenance.
5. Check retention gates for snapshot, replicas, backup/PITR, forensic hold, and
   MVCC pins.
6. Treat missing retention evidence as blocking. Do not truncate WAL on unknowns.
7. Tune batching only within bounded, observable, versioned, and disableable
   policy.
8. Resume paused work after latency and retention return below thresholds.
9. Attach incident evidence and residual risk to the release or operations
   record.

## Validation Commands

```powershell
cargo test -p andromeda-wal --tests --locked
cargo test -p andromeda-wal --test wal_durability_fence_contract --locked -- --nocapture
cargo test -p andromeda-wal --test wal_gc_four_boundaries_integration --locked -- --nocapture
cargo test -p andromeda-hadr --test wal_shipping_reclaimability_contract --locked -- --nocapture
```

## Escalate When

- Commit visibility can advance before durable WAL.
- WAL truncation would break recovery, replica catch-up, PITR, MVCC, or forensic
  hold.
- Critical queues depend on benchmark, analytics, GPU, or statistics work.
- The mitigation cannot produce audit or recovery evidence.
