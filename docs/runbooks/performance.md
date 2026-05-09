# Performance Runbook

## Purpose

Use this runbook for slow queries, slow clients, result stream backpressure,
benchmark triage, plan-cache suspicion, and resource pressure that is not yet a
WAL-pressure or replica-lag incident.

## Principles

- Benchmark and diagnostic output is advisory. It cannot replace correctness,
  durability, recovery, catalog, security, or audit gates.
- Slow clients must not block WAL, rollback, recovery, catalog publication,
  security admission, or audit evidence.
- Temporary buffers, RAM, benchmark results, GPU output, and spooled responses
  are not durable truth.

## Procedure

1. Capture scope: request id, session id, principal, surface, Procedure
   contract, workload id, current latency, error count, queue depth, and resource
   limits.
2. Separate Application Surface traffic from Administration and HA/DR controls.
3. Check whether WAL flush, recovery, audit, security admission, or catalog
   publication queues are affected. If yes, follow `wal-pressure.md` or the
   relevant recovery runbook.
4. For slow clients, reduce batch size, pause low-priority delivery, enforce
   explicit quotas, and close sessions only with audit evidence.
5. For slow queries, inspect plan-cache status, catalog statistics freshness,
   lock contention, buffer pool pressure, and storage latency.
6. Run benchmarks only to compare against a retained baseline. Record workload,
   duration, samples, hardware profile, budget, and result.
7. Do not raise budgets or accept regressions without a reviewer disposition and
   correctness gate status.
8. Resume normal service only after queues, quotas, and error rates return below
   policy thresholds.

## Validation Commands

Use applicable owner suites when code changes affect performance behavior:

```powershell
cargo test -p andromeda-bench --all-targets
cargo test -p andromeda-cli --test benchmark_cli_commands --all-features
cargo test -p andromeda-quic --test transport_contract --locked -- --nocapture
cargo test -p andromeda-exec --test result_stream_backpressure --locked -- --nocapture
```

## Escalate When

- Slow-client handling can stall durable commit or recovery.
- Memory or temp storage grows without quotas.
- Benchmark evidence is used as optimizer truth or release proof.
- Administrative or HA/DR controls are reachable through application traffic.
