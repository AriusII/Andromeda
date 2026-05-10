# andromeda-types

## Purpose

`andromeda-types` defines semantic identifiers, contract hashes, and primitive type descriptors shared across Andromeda engine crates.

Use this crate when code needs R0 value types for routing, correlation, contract identity, Procedure input or output descriptors, or ResultStream column descriptors without depending on catalog, SRPL, execution, storage, or transport owners.

## Scope

This crate owns:

- Newtype wrappers for request, session, transaction, catalog, namespace, procedure, database, invocation, and catalog-version identifiers.
- `ContractHash` as a 32-byte typed digest value.
- Scalar type descriptors for integers, decimals, floats, booleans, text, and timestamps.
- Explicit absence policy through `AbsencePolicy`.
- Per-descriptor validation that can be evaluated without catalog storage or runtime execution.

Collection-level rules, catalog object lifecycles, Procedure contracts, SRPL binding, and transport formats belong to higher-level crates.

## Non-goals

- Do not add catalog storage, DefinitionBatch behavior, Procedure contract ownership, SRPL parsing, execution planning, or transport framing.
- Do not add dynamic table names, dynamic predicates, shape-shifting returns, or implicit null semantics.
- Do not treat identifier wrappers as complete domain validation. Owners decide whether zero or any other sentinel is valid for active runtime use.
- Do not add persistent or network serialization of Rust native structs.
- Do not add dependencies on catalog, contract, SRPL, storage, execution, RPC, benchmark, analytics, or GPU crates.

## Allowed Dependencies

`andromeda-types` may depend on:

- `andromeda-error`
- The Rust standard library

No other workspace or external dependency is allowed without an ADR and topology validation update.

## Invariants

- `unsafe` code is forbidden by `src/lib.rs`.
- Identifier newtypes preserve their supplied `u64` values exactly.
- `ContractHash` is exactly 32 bytes.
- `ContractHash::zero()` remains the reserved no-contract sentinel.
- Absence must be explicit through `AbsencePolicy`; silent optionality is not allowed.
- `ScalarType::permits_silent_conversion()` must remain false unless a future ADR defines an explicit conversion policy.
- Float descriptors must not back exact relational invariants.
- Cross-column rules such as ordinal density and name uniqueness stay with the owner of the column collection.

## Prerequisites

Before changing this crate:

1. Read `../../AGENTS.md`.
2. Read `../AGENTS.md`.
3. Check `../README.md` and `../../docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md` for R0 dependency rules.
4. Review current validation tests in `src/ids.rs` and `src/types.rs`.

## Procedure

To use this crate:

1. Choose the semantic identifier newtype instead of passing a bare `u64`.
2. Use `ContractHash::from_slice` when byte length must be validated.
3. Build `TypeDescriptor::required` or `TypeDescriptor::optional` to make absence explicit.
4. Call `validate()` before publishing a descriptor into a higher-level contract or catalog owner.

To extend this crate:

1. Add only descriptors that are runtime-free and broadly shared.
2. Keep validation local to individual values unless the crate also owns the whole collection.
3. Add tests for every new invariant or error condition.
4. Recheck dependency topology if `Cargo.toml` changes.

## Validation

Run the focused crate test after changing source or documentation that describes source behavior:

```powershell
cargo test -p andromeda-types
```

Run the topology guard if dependencies or crate-boundary text changes:

```powershell
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

## Troubleshooting

| Symptom | Action |
|---|---|
| A caller needs catalog-wide uniqueness or lifecycle validation. | Implement that validation in the catalog or contract owner, not in this crate. |
| A caller wants implicit null handling. | Use `AbsencePolicy::ExplicitOptional` and keep absence visible in the contract. |
| A caller wants to serialize a descriptor. | Add an explicit codec in the owning protocol, catalog, or storage crate. |

## References

- `Cargo.toml`
- `src/lib.rs`
- `src/ids.rs`
- `src/types.rs`
- `../README.md`
- `../../docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md`
