# Rust 2024 Baseline

## Status

Accepted

## Context

Andromeda is a Rust workspace with many domain crates. A single edition policy
keeps crate boundaries, generated code, tooling, and release validation aligned.

## Decision

All workspace crates use Rust 2024 edition. The workspace MSRV is governed by
the dedicated toolchain policy ADR.

## Consequences

- New crates must declare `edition = "2024"`.
- Tooling and CI must validate the workspace with the governed stable toolchain.
- Edition changes require a new ADR and a workspace-wide migration plan.

## Validation

- `cargo metadata --no-deps --format-version 1`
- `cargo check --workspace --all-targets --all-features`

## References

- MSRV policy: `docs/adr/ADR-0017-rust-toolchain-msrv-policy.md`
