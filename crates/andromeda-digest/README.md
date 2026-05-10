# andromeda-digest

## Purpose

`andromeda-digest` provides deterministic SHA-256 digest primitives for Andromeda foundation and contract-safe crates.

Use this crate when code needs the canonical 32-byte digest backend for contract hashes, descriptor hashes, policy-version fingerprints, or other stable byte-derived identities.

## Scope

This crate owns:

- A safe Rust `Sha256` streaming hasher.
- A `sha256(bytes)` one-shot helper.
- Deterministic digest behavior that can be reused without pulling higher-level crates into R0.

The crate supplies a digest backend only. It does not decide what bytes are canonical for a contract, descriptor, catalog object, WAL record, or network frame.

## Non-goals

- Do not add ad hoc hash mixers, checksum substitutes, random identifiers, key derivation, password hashing, MACs, or signature algorithms.
- Do not own Procedure contract canonicalization, catalog object canonicalization, or StructuredObject descriptor layout.
- Do not serialize Rust native structs directly to generate digests.
- Do not add runtime GPU, SIMD, benchmark, analytics, RPC, storage, catalog, or SRPL dependencies.
- Do not treat digest output as storage truth unless the owning durable component has explicit codec and recovery evidence.

## Allowed Dependencies

`andromeda-digest` has no workspace dependencies.

Allowed dependency policy:

- Use the Rust standard library only.
- Keep the implementation dependency-free unless an ADR replaces the digest backend and updates topology validation.
- Do not add crypto-provider, GPU, SIMD, benchmark, or analytics dependencies in this crate.

## Invariants

- `unsafe` code is forbidden by `src/lib.rs`.
- `Sha256::finalize()` must always return exactly 32 bytes.
- Streaming updates must produce the same digest as the one-shot helper for the same byte sequence.
- Digest inputs must be caller-provided canonical bytes, not implicit Rust memory layout.
- Digest behavior must remain deterministic across platforms.
- Any future backend change must preserve byte-for-byte compatibility or provide an explicit migration decision.

## Prerequisites

Before changing this crate:

1. Read `../../AGENTS.md`.
2. Read `../AGENTS.md`.
3. Check `../README.md` and `../../docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md` for R0 dependency rules.
4. Review the FIPS test vectors in `src/digest.rs`.

## Procedure

To use this crate:

1. Build canonical byte input in the owning crate.
2. Pass those bytes to `sha256(bytes)` for one-shot hashing, or use `Sha256` for streaming input.
3. Store or transmit the digest through the owning component's explicit format.

To extend this crate:

1. Add tests from published vectors for any digest behavior change.
2. Keep the public API small and backend-focused.
3. Document compatibility impact before changing digest output bytes.

## Validation

Run the focused crate test after changing source or documentation that describes source behavior:

```powershell
cargo test -p andromeda-digest
```

Run the topology guard if dependencies or crate-boundary text changes:

```powershell
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

## Troubleshooting

| Symptom | Action |
|---|---|
| Two components produce different hashes for the same concept. | Check canonical byte construction in the owning components before changing this crate. |
| A caller wants a checksum for corruption detection. | Use or design the checksum format in the storage, WAL, or protocol owner instead of overloading SHA-256 semantics. |
| A caller wants accelerated hashing. | Keep acceleration outside this foundation crate unless an ADR defines a deterministic backend with scalar fallback and validation. |

## References

- `Cargo.toml`
- `src/lib.rs`
- `src/digest.rs`
- `../README.md`
- `../../docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md`
