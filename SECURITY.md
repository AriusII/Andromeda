# Security Policy

## Supported branch

The `main` branch is the only supported development branch during the current Andromeda phase.

## Security principles

- No application-facing ad hoc SQL.
- No gRPC protocol surface.
- QUIC-only transport.
- Strongly typed contracts.
- WAL-before-visible-commit.
- Explicit auditability for critical decisions.
- Unsafe Rust must be isolated, documented, fuzzed, and reviewed.

## Dependency security

Dependency updates are managed through Dependabot and reviewed through GitHub dependency review, cargo-audit, and cargo-deny.
