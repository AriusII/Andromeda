# andromeda-resource

## Purpose

`andromeda-resource` provides typed resource budgets and limits with checked constructors.

Use this crate when a component needs to carry byte budgets, byte limits, stream limits, or aggregate resource limits without owning allocation, scheduling, hardware detection, or runtime enforcement.

## Scope

This crate owns:

- `ByteBudget`, `ByteLimit`, and `StreamLimit` checked value types.
- `ResourceBudget` for declared memory and temporary byte budgets.
- `ResourceLimits` for aggregate memory, temporary storage, and stream limits.
- Typed resource-limit errors for zero values, overflow, and budget-limit violations.

The crate models declared limits and simple admission checks. It does not allocate resources or reserve capacity.

## Non-goals

- Do not make this crate a scheduler, allocator, memory manager, stream runtime, hardware detector, benchmark owner, or GPU execution policy owner.
- Do not treat declared RAM, temporary storage, or stream budgets as durable truth.
- Do not add WAL, storage, catalog, SRPL, RPC runtime, execution, benchmark, analytics, or GPU dependencies.
- Do not use resource admission as a substitute for WAL durability, catalog publication, recovery validation, or security authorization.

## Ownership

`andromeda-resource` owns checked resource-limit primitives and local validation.

Runtime owners remain responsible for measuring actual resource availability, enforcing limits, applying backpressure, emitting observability evidence, and integrating with security or admission policy. Keep this crate small and runtime-free.

## Validation

For documentation-only changes, check that this README keeps the required headings and preserves the primitive-only boundary.

For source changes in this crate, prefer:

```powershell
cargo test -p andromeda-resource
```

Run workspace topology validation if dependencies or crate-boundary claims change.

## References

- `Cargo.toml`
- `src/lib.rs`
- `src/limits.rs`
- `src/error.rs`
- `../README.md`
- `../../AGENTS.md`
