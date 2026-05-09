# Extraction Status

Last refreshed: 2026-05-09.

This page summarizes the current crate extraction and documentation
consolidation status for readers entering through `/docs`.

## Current State

| Area | Status |
| --- | --- |
| Workspace size | 88 active crates. |
| Documentation root | `/docs` is canonical. |
| Legacy documentation | Removed from active repository navigation after link migration. |
| Transaction family | Dedicated owner crates exist for transaction state, MVCC, locking, savepoints, and transaction log concerns. |
| WAL and storage family | Dedicated owner crates exist for WAL, WAL codec, page, heap, index, segment, manifest, recovery, backup, and restore concerns. |
| Protocol family | Dedicated owner crates exist for protocol, codec, QUIC runtime, RPC, and result stream concerns. |
| Security and audit family | Dedicated owner crates exist for principal, admission, IAM, security, security contracts, audit, decision trace, and observability concerns. |
| SRPL and procedure family | Dedicated owner crates exist for SRPL lexer, parser, AST, binder, lowering, IR, diagnostics, cardinality, execution adapter, interpreter, contracts, store, and runtime concerns. |

## Boundaries

- This status page records current structure; it does not prove runtime
  readiness.
- Any crate-count or readiness claim should be refreshed from current command
  output before release decisions.

## Remaining Work

- Finish direct-owner caller cleanup where broad facades still obscure
  ownership.
- Keep architecture and specification links rooted in `/docs`.
- Complete targeted validation for each subsystem before making readiness
  claims.
- Keep tool inventories aligned with `/docs` and `tools/loom-models`.
