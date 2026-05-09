# andromeda-core

## Purpose

`andromeda-core` is a temporary compatibility facade over Andromeda foundation crates and principal identity primitives.

Use this crate when existing code still depends on the historical `andromeda_core::*` public paths during the foundation split. New code should prefer narrower owner crates when an appropriate crate already exists.

## Scope

This crate currently exposes:

- Compatibility reexports for digest, error, hardware, time, and type descriptors.
- Compatibility modules for historical `andromeda_core::digest::*` and `andromeda_core::policy::*` imports.
- Principal identity, certificate identity, roles, and permission sets.
- Compatibility reexports for principal permissions and surface scopes owned by `andromeda-security-contract`.
- Principal registry authorization evidence, denial reasons, policy evidence binding, and decision metadata.

The crate preserves public import compatibility while downstream crates migrate to smaller ownership boundaries.

## Non-goals

- Do not move foundation ownership back into this crate when a narrower crate already owns the concept.
- Do not treat this facade as a durable IAM store, certificate parser, mTLS runtime, audit ledger, Procedure dispatcher, catalog owner, WAL owner, storage owner, or recovery authority.
- Do not expose Administration, security, recovery, backup, or cluster behavior through an Application Surface.
- Do not add application-facing ad hoc SQL or bypass typed Procedure contracts.
- Do not serialize Rust native structs directly to disk or network.

## Ownership

`andromeda-core` owns compatibility reexports and the current principal model surface that has not yet been split into a narrower owner crate. Runtime-free principal permission and surface-scope vocabulary should be changed in `andromeda-security-contract`, then reexported here only for legacy import compatibility.

When changing this crate, keep `lib.rs` thin, keep public API additions intentional, and prefer migration toward narrower foundation or security crates over broadening the facade. Authorization evidence in this crate is policy evidence for evaluation and audit; it is not durable database truth or release-readiness evidence by itself.

## Validation

For documentation-only changes, check that this README keeps the required headings and describes `andromeda-core` as a compatibility facade.

For source changes in this crate, prefer:

```powershell
cargo test -p andromeda-core
```

Run workspace topology validation if dependencies, public reexports, or facade-boundary claims change.

## References

- `Cargo.toml`
- `src/lib.rs`
- `src/principal/mod.rs`
- `tests/foundation_facade_compat.rs`
- `tests/principal_contract_projection.rs`
- `../README.md`
- `../../AGENTS.md`
