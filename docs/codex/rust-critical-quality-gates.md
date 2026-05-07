# Rust Critical Quality Gates

## Purpose

Define validation gates for Rust work in Andromeda.

## Standard gate

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
cargo test --doc --workspace
```

## Supply-chain gate

```bash
cargo audit
cargo deny check
cargo vet
```

## C4/C5 gate

For WAL, recovery, storage, catalog, RPC, security, or transaction work:

```text
property tests
fuzz tests
Miri where applicable
crash/recovery matrix
forensic startup test
backup/restore test
```

## Failure policy

A C4/C5 validation failure is a blocker unless the change is explicitly experimental and isolated.
