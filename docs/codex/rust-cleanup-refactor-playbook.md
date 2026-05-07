# Rust Cleanup and Refactor Playbook

## Purpose

Guide broad code cleanup, refactor, dead code removal, and architecture consolidation.

## Procedure

1. Stabilize behavior with tests.
2. Map crates and modules.
3. Identify dead code and orphans.
4. Reduce visibility.
5. Split oversized files by responsibility.
6. Remove stale dependencies and features.
7. Run validation.
8. Document decisions.

## Required commands

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo tree -d
cargo tree -e features
```

Optional:

```bash
cargo machete
cargo +nightly udeps --workspace --all-targets
cargo deny check
cargo audit
cargo nextest run --workspace --all-features
```

## Refactor rejection criteria

Reject a refactor if it:

- changes behavior without tests,
- increases public API surface unnecessarily,
- hides cost,
- introduces unreviewed unsafe,
- mixes feature work and cleanup without separation,
- removes tests instead of fixing them.
