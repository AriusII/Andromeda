# Copilot Governance

Copilot may assist with code generation, review, and documentation. It must not override Andromeda invariants.

Copilot must not suggest:

- gRPC usage.
- Application-facing ad hoc SQL.
- Weakening WAL, MVCC, recovery, audit, or Procedure contracts.
- Unbounded unsafe Rust.
- Hidden runtime behavior that cannot be tested, observed, or disabled.
