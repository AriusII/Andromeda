---
name: catalog-definitionbatch
description: "Use for Catalog, Modelization, DefinitionBatch, DryRun and CatalogVersion work."
category: andromeda-catalog
---

# catalog-definitionbatch

## When to use
Catalog definitions, imports, migrations or object lifecycle are involved.

## Purpose
Use for Catalog, Modelization, DefinitionBatch, DryRun and CatalogVersion work.

## Process
- Ensure objects are versioned and dependency-ordered.
- DryRun must detect conflicts, destructive changes, contract incompatibility and security downgrade.
- Apply must be transactional, WAL-covered and all-or-nothing.
- No demi-catalogue publication.

## Expected output
- Catalog/DefinitionBatch plan or audit.

## Guardrails
- No mutable-in-place catalog changes.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
