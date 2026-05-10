# andromeda-time

## Purpose

`andromeda-time` defines deterministic engine timestamp primitives and clock abstractions for Andromeda crates.

Use this crate when code needs a shared timestamp value, a real-time clock adapter, or a manual clock for deterministic tests without depending on higher-level runtime crates.

## Scope

This crate owns:

- `EngineTimestamp`, represented as milliseconds since the Unix epoch.
- Checked and saturating millisecond arithmetic for engine timestamp values.
- The `Clock` trait.
- `SystemClock` for wall-clock reads through `std::time::SystemTime`.
- `ManualClock` for deterministic tests and controlled time advancement.

This crate provides time values and clock abstraction only. Transaction ordering, deadline policy, distributed clock consensus, recovery sequencing, and scheduler behavior belong to their owning crates.

## Non-goals

- Do not own MVCC visibility, commit ordering, WAL durability, lock waits, retry budgets, or deadline enforcement.
- Do not introduce async runtime dependencies, timer wheels, schedulers, or transport timers.
- Do not add timezone, calendar, localization, or human date formatting policy.
- Do not serialize Rust native timestamp structs directly to disk or network.
- Do not treat wall-clock readings as durable truth for commit, recovery, catalog publication, or security decisions.

## Allowed Dependencies

`andromeda-time` may depend on:

- `andromeda-error`
- The Rust standard library

No other workspace or external dependency is allowed without an ADR and topology validation update.

## Invariants

- `unsafe` code is forbidden by `src/lib.rs`.
- `EngineTimestamp` preserves a `u64` millisecond value.
- `EngineTimestamp::checked_add_millis` must return `None` on overflow.
- `EngineTimestamp::saturating_add_millis` must saturate at `EngineTimestamp::MAX`.
- `ManualClock` changes only when `set` or `advance_millis` is called.
- `ManualClock::advance_millis` must leave the clock unchanged when overflow is rejected.
- `SystemClock` is an adapter for current wall-clock time, not a source of durable engine truth.

## Prerequisites

Before changing this crate:

1. Read `../../AGENTS.md`.
2. Read `../AGENTS.md`.
3. Check `../README.md` and `../../docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md` for R0 dependency rules.
4. Review current timestamp and clock tests in `src/time.rs`.

## Procedure

To use this crate:

1. Accept a `Clock` where deterministic behavior matters.
2. Use `SystemClock` only at runtime boundaries that are allowed to observe wall-clock time.
3. Use `ManualClock` in tests instead of sleeping or depending on real time.
4. Use checked arithmetic when overflow must be reported, and saturating arithmetic only when saturation is the intended policy.

To extend this crate:

1. Keep additions runtime-free and deterministic.
2. Add tests for overflow, monotonic expectations, and manual-clock stability.
3. Keep policy decisions such as retries, deadlines, and transaction ordering in the owning subsystem.

## Validation

Run the focused crate test after changing source or documentation that describes source behavior:

```powershell
cargo test -p andromeda-time
```

Run the topology guard if dependencies or crate-boundary text changes:

```powershell
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

## Troubleshooting

| Symptom | Action |
|---|---|
| A test is flaky because it waits on real time. | Inject `ManualClock` and advance it explicitly. |
| A caller needs a deadline or retry budget. | Implement the policy in the transport, execution, or transaction owner and use this crate only for timestamp values. |
| A timestamp must be written to disk or sent over the network. | Encode the millisecond value through the owning component's explicit byte contract. |

## References

- `Cargo.toml`
- `src/lib.rs`
- `src/time.rs`
- `../README.md`
- `../../docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md`
