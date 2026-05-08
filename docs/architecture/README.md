# Architecture

This directory contains high-level architectural documentation for Andromeda.

## Purpose

Architecture documents describe:
- System design and module boundaries
- Engine responsibilities and subsystem architecture
- Anti-patterns and what NOT to do
- Cross-document consistency and invariants

## Contents

### System Design

- **module-boundaries.md** - Crate ownership, dependencies, anti-patterns
- **engine-responsibilities.md** - What each engine component owns
- **subsystem-architecture.md** - Deep dives into specific subsystems (WAL, storage, catalog, SRPL, RPC, MVCC)

### Core Invariants

- **invariants.md** - Non-negotiable project rules
- **transaction-model.md** - MVCC, isolation levels, recovery semantics
- **wal-contract.md** - Write-ahead log design and durability guarantees
- **procedure-model.md** - Typed procedure contracts and execution

### Integration Points

- **rpc-surface.md** - QUIC-only RPC protocol, no gRPC
- **catalog-contract.md** - Catalog descriptors, versioning, dependencies
- **security-model.md** - Authentication, authorization, audit trails

## Cross-References

- ADRs (docs/adr/) for decision history
- Specifications (docs/specifications/) for formal specs
- Runbooks (docs/runbooks/) for operational guidance

## Anti-Patterns

DO NOT:
1. Introduce ad hoc SQL (procedures only)
2. Bypass typed procedure contracts
3. Commit before durable WAL
4. Place GPU work in commit/WAL/recovery paths
5. Serialize Rust structs directly to disk
6. Use dynamic table names/implicit null semantics in SRPL
7. Expose Admin/HA capabilities through Application Surface
