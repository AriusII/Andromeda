# Rust Engineering Standard

## Scope

Use for Rust implementation work in Andromeda.

## Requirements

- Prefer safe Rust.
- Encapsulate `unsafe` behind safe APIs.
- Document memory and aliasing invariants near the `unsafe` block.
- Use explicit error types for engine boundaries.
- Avoid panics in server-critical paths.
- Use property tests and fuzz tests for binary formats.
- Keep crate boundaries aligned with engine boundaries.

## Required review

Any code touching WAL, pages, MVCC, catalog mutation, QUIC framing, or cryptographic material requires a dedicated
review.
