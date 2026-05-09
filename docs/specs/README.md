# Andromeda Specifications

This directory is the canonical `/docs` surface for subsystem specifications.
The files here consolidate the artifact-level contracts into domain documents
that are short enough to review and stable enough to cite.

## Domain specs

| Spec | Covers |
| --- | --- |
| [Core Contracts](core-contracts.md) | Type descriptors, Procedure contracts, canonical hashes, StructuredObject payload boundaries, and compatibility rules. |
| [Catalog And SRPL](catalog-srpl.md) | Catalog object identity, DefinitionBatch, catalog diffs, SRPL-to-contract publication, dependency checks, and catalog WAL evidence. |
| [Storage And WAL](storage-wal.md) | WAL bytes, page bytes, segment indexes, database manifests, buffer pool policy, and storage publication gates. |
| [Transaction And Recovery](transaction-recovery.md) | Durable commit and rollback evidence, MVCC visibility, recovery reports, recovery traces, and Procedure invocation traces. |
| [RPC, Security, And Audit](rpc-security-audit.md) | QUIC RPC frames, ResultStream ordering, surface separation, security admission, DecisionTrace, and AuditLedger rules. |
| [HA/DR, Backup, And Restore](hadr-backup-restore.md) | Quorum, fencing, WAL shipping, promotion, backup manifests, PITR, restore validation, and retention. |
| [Advisory Optimizer And Hardware](advisory-optimizer-hardware.md) | Statistics, plan cache keys, Map descriptors, refresh validation, GPU policy, hardware policy, and advisory evidence boundaries. |

## Global invariants

- Durable database truth is the latest accepted snapshot or manifest plus the
  validated durable WAL required from that point.
- Audit, traces, plans, benchmarks, GPU output, and statistics are evidence.
  They are not database truth unless a domain spec explicitly routes them
  through durable publication and validation gates.
- Persistent and network formats use explicit codecs, version fields, byte
  order markers, length bounds, and checksum or digest evidence. Rust memory
  layout is never a durable or network format.
- Procedure invocation, catalog publication, commit visibility, rollback
  completion, restore acceptance, and HA/DR promotion must fail closed when
  required evidence is missing or inconsistent.
- Application, Administration, Monitoring, BackupAgent, Cluster or HA/DR, and
  Forensic surfaces must stay separated. Privileged or cluster operations must
  not be tunneled through the Application surface.

## Compatibility

These specs preserve the existing v0 contracts unless they explicitly name a
pending gap. A breaking byte-format, hash-input, surface, or durability change
requires a new spec version or a decision record before implementation.
