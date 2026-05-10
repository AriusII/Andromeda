# Loom Test Models

## Purpose

This directory owns a standalone Loom model crate for small bounded concurrency checks. It is intentionally outside the root Cargo workspace.

The current model closes the concrete-path gap for Loom evidence by making the WAL durable-before-visible publication rule executable. It is intentionally narrow and does not claim release readiness.

Cargo build output under `tools/loom-models/target/` is a local generated artifact covered by `tools/loom-models/.gitignore`. Do not commit it, move it into repository tooling, or treat it as release evidence.

## Scope

Use this crate for minimal, deterministic Loom models that are not yet wired into an owning engine crate.

Current model:

| Model path | Safety property | Modeled state | Bound |
| --- | --- | --- | --- |
| `tools/loom-models/tests/wal_durable_before_visible.rs` | A visible commit LSN is never observed before the corresponding WAL LSN is durable. | Two Loom atomics: `durable_lsn` and `visible_lsn`. | One publisher thread and one observer thread. |

## Non-goals

- Do not treat this standalone model as a replacement for owner-crate tests.
- Do not use this model as proof of WAL append, fsync, crash/recovery, MVCC visibility, Procedure dispatch, audit, or global release readiness.
- Do not commit or depend on `tools/loom-models/target/`; it is disposable local Cargo output.
- Do not use sleeps, wall-clock timing, retries, or scheduler luck as validation.
- Do not model GPU, benchmark, or temporary output as a critical source of truth.
- Do not weaken WAL, MVCC visibility, typed Procedure, IAM, audit, or recovery invariants.

## Prerequisites

- Rust 2024-compatible toolchain.
- Network or a populated Cargo cache for the `loom` crate.
- Run commands from the repository root unless otherwise stated.

## Procedure

1. Identify the shared state or async boundary that needs model checking.
2. Define the failure that Loom must make observable.
3. Keep the model bounded and deterministic.
4. Record the model path, exact command, explored boundary, and residual risk.
5. Move or duplicate the model into an owning crate when production code exposes the modeled boundary behind a stable test adapter.

## Acceptance Criteria

- Each Loom entry names the concurrency risk, modeled state, thread or task bound, and exact command.
- The model has no dependence on sleeps, wall-clock time, retries, external services, or hidden process state.
- The assertion proves a safety property such as no lost wakeup, no double publication, no visibility-before-durability, or no shutdown leak.
- Any unmodeled behavior is stated as residual risk.
- C5 concurrency models link to the deterministic crash/recovery, WAL, or security gate that remains authoritative.

## Validation

Run:

```powershell
cargo test --manifest-path tools/loom-models/Cargo.toml --locked
```

Expected tests:

| Test | Expected result | Meaning |
| --- | --- | --- |
| `visible_commit_is_never_observed_before_durable_wal` | Pass | All explored interleavings preserve durable WAL before visible commit. |
| `broken_publication_order_is_detected_by_loom` | Pass through `should_panic` | Loom observes the intentionally inverted publication order and detects the invariant violation. |

## Troubleshooting

If the state space is too large, reduce the model to the publication or synchronization boundary that owns the safety property. Do not replace the model with retries.

If the model requires production-only behavior that cannot be bounded, split the code so the synchronization primitive can be tested behind a small model adapter.

## References

- `tests/README.md`
- `docs/testing/FUZZING_PLAN.md`
- `docs/testing/RELEASE_GATES.md`
