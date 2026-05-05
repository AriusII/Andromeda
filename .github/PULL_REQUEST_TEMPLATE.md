# Pull Request

## Summary

Describe the change and its scope.

## Andromeda invariants

- [ ] No application-facing ad hoc SQL was introduced.
- [ ] No gRPC dependency or service contract was introduced.
- [ ] QUIC-only protocol boundaries are preserved.
- [ ] WAL-before-visible-commit remains preserved.
- [ ] Procedure-only execution model remains preserved.
- [ ] Unsafe Rust is documented, isolated, and tested.
- [ ] Tests were added or updated.

## Validation

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

## Risk

Describe correctness, recovery, security, and performance risk.
