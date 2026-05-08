# andromeda-policy

## Purpose

`andromeda-policy` provides runtime-free policy identity, policy version, and admission stance primitives.

Use this crate when a component needs small typed policy identifiers or admission labels without pulling in IAM runtime, storage, transport, or security-contract ownership.

## Scope

This crate owns:

- `PolicyId` as a checked policy identifier.
- `PolicyVersion` as a fixed-width version value.
- `AdmissionStance` as a bounded admission decision label.

The crate supplies primitive vocabulary only. It does not evaluate live access requests or store mutable policy state.

## Non-goals

- Do not make this crate an IAM runtime, authorization evaluator, role store, policy database, certificate parser, revocation source, audit ledger, or transport boundary.
- Do not duplicate the broader security vocabulary owned by `andromeda-security-contract`.
- Do not expose Administration, recovery, security management, backup, or cluster authority through the Application Surface.
- Do not add application-facing ad hoc SQL or bypass typed Procedure contracts.
- Do not treat policy identifiers or versions as durable truth without owning storage and recovery evidence.

## Ownership

`andromeda-policy` owns compact, runtime-free policy primitives.

Runtime security, admission, catalog, audit, and storage owners remain responsible for policy evaluation, policy publication, evidence persistence, revocation, and operator-visible decisions. Keep this crate dependency-light and avoid broadening it into a policy engine.

## Validation

For documentation-only changes, check that this README keeps the required headings and preserves the primitive-only boundary.

For source changes in this crate, prefer:

```powershell
cargo test -p andromeda-policy
```

Run workspace topology validation if dependencies or ownership claims change.

## References

- `Cargo.toml`
- `src/lib.rs`
- `src/identity.rs`
- `src/admission.rs`
- `../README.md`
- `../../AGENTS.md`
