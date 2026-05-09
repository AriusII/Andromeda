# andromeda-core

## Purpose

`andromeda-core` is a drained compatibility crate. Its former foundation,
hardware, digest, time, principal, security-contract, and error re-exports now
live only in their owner crates.

Use direct owner crates for all new code and migrations.

## Scope

This crate currently exposes:

- No public re-exports.
- No storage, execution, recovery, transport, or catalog runtime behavior.
- No runtime dependencies.

The crate remains as a workspace member while downstream migration history and
topology documentation catch up to the drained compatibility surface.

## Non-goals

- Do not move foundation ownership back into this crate when a narrower crate already owns the concept.
- Do not treat this crate as a durable IAM store, certificate parser, mTLS runtime, audit ledger, Procedure dispatcher, catalog owner, WAL owner, storage owner, or recovery authority.
- Do not expose Administration, security, recovery, backup, or cluster behavior through an Application Surface.
- Do not add application-facing ad hoc SQL or bypass typed Procedure contracts.
- Do not serialize Rust native structs directly to disk or network.

## Ownership

`andromeda-core` owns no active foundation vocabulary. Principal identity,
session, permission-set, registry, and policy-evidence primitives should be
changed in `andromeda-principal`. Runtime-free principal permission and
surface-scope vocabulary should be changed in `andromeda-security-contract`.

When changing this crate, keep `lib.rs` thin and prefer migration toward
narrower foundation or security crates over reintroducing a broad compatibility
surface.

## Validation

For documentation-only changes, check that this README keeps the required
headings and describes `andromeda-core` as drained.

For source changes in this crate, prefer:

```powershell
cargo test -p andromeda-core
```

Run workspace topology validation if dependencies, public exports, or
compatibility-boundary claims change.

## References

- `Cargo.toml`
- `src/lib.rs`
- `../andromeda-error/src/lib.rs`
- `../andromeda-types/src/lib.rs`
- `../andromeda-hardware/src/lib.rs`
- `../andromeda-principal/src/lib.rs`
- `../andromeda-security-contract/src/lib.rs`
- `../README.md`
- `../../AGENTS.md`
