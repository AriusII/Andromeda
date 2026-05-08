# andromeda-codec

## Purpose

`andromeda-codec` provides explicit little-endian byte helpers for runtime-free binary contracts.

Use this crate when an Andromeda component needs bounded reads or writes for primitive unsigned integers and exact byte slices without relying on Rust native memory layout.

## Scope

This crate owns:

- Little-endian helpers for `u8`, `u16`, `u32`, `u64`, and `u128`.
- Exact byte read and write helpers over caller-provided slices.
- Cursor and slice-bound validation for fixed-width binary fields.
- Typed codec errors for bounds, cursor overflow, and invalid input.

The crate is a low-level helper library. It does not define the full byte format for WAL records, pages, manifests, RPC frames, catalog descriptors, Procedure contracts, or StructuredObject payloads.

## Non-goals

- Do not serialize Rust native structs directly to disk or network.
- Do not add runtime I/O, async transport, filesystem, WAL, storage, catalog, SRPL, benchmark, analytics, or GPU responsibilities.
- Do not decide canonical field order, versioning, checksums, hash inputs, or compatibility policy for higher-level formats.
- Do not treat successful slice encoding as durability or recovery evidence.

## Ownership

`andromeda-codec` owns byte-level helpers and typed codec errors only.

Owning crates remain responsible for their explicit format specifications, version fields, checksums, compatibility rules, golden tests, and crash/recovery validation. If a higher-level component needs a new durable or network format, define that contract in the owning crate and use this crate only for primitive byte operations.

## Validation

For documentation-only changes, check that this README keeps the required headings and does not claim ownership of durable formats.

For source changes in this crate, prefer:

```powershell
cargo test -p andromeda-codec
```

Run workspace topology validation if dependencies or crate-boundary claims change.

## References

- `Cargo.toml`
- `src/lib.rs`
- `src/little_endian.rs`
- `src/error.rs`
- `../README.md`
- `../../AGENTS.md`
