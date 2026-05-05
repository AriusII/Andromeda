# Andromeda Copilot Instructions

This repository implements Andromeda, a mission-critical transactional relational database system written in Rust.

## Non-negotiable project rules

- Use Rust-first implementation patterns.
- Use QUIC transport only.
- Protobuf contracts are allowed for schema and message contracts.
- gRPC is forbidden.
- Application-facing ad hoc SQL is forbidden.
- Procedure-only execution is the behavioral model.
- SRPL is the strict relational procedure language.
- Every Procedure must be typed, contractual, observable, and transaction-scoped.
- No visible commit is valid before durable WAL.
- The GPU must never participate in the commit path.
- Unsafe Rust must be encapsulated behind safe APIs, documented, fuzzed, and reviewed.
