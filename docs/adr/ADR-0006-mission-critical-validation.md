# Mission-Critical Validation

## Status

Accepted

## Context

Durability, recovery, security, and transport boundaries require stronger proof
than compilation alone. Release claims need direct owner tests and retained
evidence for the exact commands that were run.

## Decision

Critical paths require owner tests, topology checks, recovery or replay evidence
when durability is involved, and release-gate summaries that include command,
toolchain, commit, pass/fail status, skipped scope, and unresolved gaps.

## Consequences

- Runtime facades do not replace owner-crate tests.
- Durable truth requires WAL, recovery, and crash-path evidence.
- Security-sensitive paths require audit and permission boundary coverage.
- Transport and protocol changes require wire-format and projection gates.

## Validation

- `cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture`
- `cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture`
- `cargo check --workspace --all-targets --all-features`

## References

- Testing strategy: `docs/testing/testing-strategy.md`
- Release gates: `docs/testing/release-gates.md`
- Governance gates: `docs/governance/release-gates.md`
