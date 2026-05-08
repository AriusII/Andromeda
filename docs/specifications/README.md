# Specifications

This directory contains formal specifications and design documents for Andromeda subsystems.

## Purpose

Specifications provide:
- Formal semantics and contracts
- Detailed algorithm descriptions
- Protocol specifications with examples
- Data structure and layout specifications

## Contents

### Catalog

- **catalog-object-model.md** - Catalog descriptors, versions, policies, state lifecycles
- **definition-batch-specification.md** - DefinitionBatch dry-run, apply, rollback behavior
- **modelization-specification.md** - Catalog import and versioning

### Storage

- **storage-page-layout.md** - Page format, alignment, indexing
- **segment-manifest-specification.md** - Segment organization, metadata, checksums
- **coldstore-immutable-design.md** - ColdStore segments, snapshots, archive retention

### WAL (Write-Ahead Log)

- **wal-record-specification.md** - Record format, encoding, recovery
- **crash-recovery-specification.md** - Startup modes, recovery procedures, idempotency
- **mvcc-recovery-proof.md** - Proof that MVCC state recovers correctly

### Execution

- **srpl-type-system.md** - Type system, semantics, error handling
- **procedure-contract-design.md** - Typed contracts, versioning, compatibility
- **execution-trace-specification.md** - Execution observability and forensics

### RPC/QUIC

- **quic-frame-protocol.md** - QUIC frame types, state machine
- **rpc-contract-specification.md** - RPC semantics, ordering, timeout handling

### Security

- **permission-policy-matrix.md** - Permission checks by operation
- **audit-trace-specification.md** - Audit log format and events
- **threat-model.md** - Security assumptions and attack surface

## Validation

Each specification should:
- Be reviewable (not exceed GitHub's display limits)
- Reference ADRs and architecture docs
- Include examples and counter-examples
- Define formal acceptance criteria
- Link to implementation code

## Cross-References

- Architecture (docs/architecture/) for design context
- ADRs (docs/adr/) for decisions
- Runbooks (docs/runbooks/) for operational procedures
