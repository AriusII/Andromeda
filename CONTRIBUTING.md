# Contributing to Andromeda

Andromeda is currently a single-maintainer, mission-critical database engine project. Contributions must preserve project invariants.

## Required local checks

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

## Architecture rules

- Do not introduce gRPC.
- Do not introduce application-facing ad hoc SQL.
- Do not bypass Procedure contracts.
- Do not weaken WAL, recovery, or audit guarantees.
- Do not introduce unsafe Rust without a documented safety contract.
