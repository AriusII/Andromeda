# andromeda-locking

## Purpose

`andromeda-locking` is the C5 owner crate for lock coordination and conflict management.

This crate owns lock modes, lock ownership, wait queues, deadlock handling, and cleanup evidence.

## Scope

This crate owns:

- Lock identifiers, modes, grants, waits, and ownership lifecycle.
- Strict two-phase locking validation where required by the transaction protocol.
- Deadlock detection, victim selection evidence, timeout policy, and cleanup.
- Typed errors for invalid lock transitions, stale ownership, deadlock routing, and protocol violations.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No transaction commit publication, WAL byte ownership, MVCC row visibility, page storage, or recovery replay implementation.
- No Administration or HA/DR exposure through the Application Surface.
- No GPU output, benchmark output, RAM state, or temporary storage as locking truth.

## Prerequisites

Before behavior lands here:

- Lock release must not make a commit visible before the commit WAL record is durable.
- Cleanup after rollback or deadlock victim selection must preserve recovery-consistent transaction state.
- Persistent and network bytes must use explicit codecs, not Rust native struct layout.
- Concurrency-sensitive behavior must include deterministic tests and, where practical, model or property checks.

## Procedure

1. Define the lock responsibility being split.
2. Keep `src/lib.rs` limited to module declarations and intentional reexports.
3. Separate lock scheduling from transaction terminal-status authority.
4. Make deadlock and cleanup evidence observable and typed.
5. Add concurrency, cleanup, and recovery-adjacent tests before moving behavior.

## Validation

Run `cargo fmt`, `cargo check`, `cargo clippy`, focused lock-manager tests, deadlock cleanup tests, and recovery-adjacent transaction tests.

## Troubleshooting

If a lock cleanup path changes visible transaction state without durable terminal evidence, fail closed and route the issue to transaction ownership.

## References

- `src/lib.rs`
- Transaction lifecycle owner: `crates/andromeda-transaction/`
