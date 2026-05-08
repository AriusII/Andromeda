# andromeda-retry

## Purpose

`andromeda-retry` owns retry classification, retry decisions, and bounded retry policy outside `andromeda-exec`.

## Non-goals

- Executing retries on a timer or driving runtime scheduling.
- Owning transport reconnection behavior.
- Defining cataloged Procedure admission or permission semantics.
- Owning durability, WAL evidence, or commit/recovery decisions.

## Scope

The crate classifies transient and persistent errors, validates retry budgets, and records retry-attempt evidence. It does not issue retries, decide storage truth, own WAL evidence, or encode business-specific retry policy.

## Prerequisites

- Callers must classify errors through `ErrorRetryability` and pass deterministic policy data.
- Retry policies must be validated before scheduling work (`RetryPolicy::validate`).
- Retry decisions must remain observable and bounded by max attempts.
- Runtime callers are responsible for enforcing transport timeout, cancellation, and backoff execution.

## Procedure

1. Classify an invocation error using `ErrorRetryability`.
2. Compute a bounded decision with `RetryPolicy::decision_after_failure`.
3. Emit the retry attempt record (`RetryAttempt`) only through trace/audit ownership owned by the caller boundary.
4. Reject invalid policies and invalid attempt numbers as contract errors before retries are scheduled.
5. Add/adjust focused unit tests when classifying new error kinds or changing backoff math.

## Validation

```powershell
cargo test -p andromeda-retry
cargo test -p andromeda-retry --lib -- --nocapture
```

## Troubleshooting

- Unexpected immediate give-up: verify error kind mapping and whether the invocation already used the configured max attempts.
- Non-growing delay: confirm `initial_backoff_ms`, `max_backoff_ms`, and attempt index are passed correctly.
- Policy acceptance despite invalid inputs: ensure `RetryPolicy::validate` is called before decisioning.

## References

- [`Cargo.toml`](Cargo.toml)
- [`src/lib.rs`](src/lib.rs)
- [`../andromeda-exec`](../andromeda-exec)
