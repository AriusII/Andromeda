---
name: catalog-definitionbatch
description: Governs Catalog, Modelization, DefinitionBatch, DryRun, and CatalogVersion changes when prompts mention catalog publication, schema changes, or definition batches.
license: MIT
---

# catalog-definitionbatch

## When to use
- The prompt mentions Catalog, Modelization, DefinitionBatch, DryRun, CatalogVersion, schema publication, DDL-like changes, or catalog recovery.
- A change touches andromeda-catalog, andromeda-definition-batch, catalog-store, or SRPL catalog binding.
- A review must determine whether a catalog mutation is atomic and durable.

## Purpose
Ensure catalog mutation is atomic, versioned, auditable, and dry-run capable. The skill keeps modelization and DefinitionBatch publication from becoming informal in-memory edits or partially visible schema changes.

## Process
1. Read the object model, DefinitionBatch spec, and publication ADR before changing catalog behavior.
2. Classify the request as validation-only DryRun, durable publication, compatibility check, or recovery/replay.
3. Ensure every visible catalog change receives a new CatalogVersion and publication evidence.
4. Keep batch validation deterministic and reject partial publication, ordering ambiguity, and unversioned mutation.
5. Add tests for dry-run results, successful publication, rejection, recovery replay, and digest/compatibility behavior.

## Expected output
- A DefinitionBatch flow showing validate, dry-run, publish, persist, notify, and recover steps.
- CatalogVersion and audit/recovery evidence expectations.
- Owned tests and failure cases required before merge.

## Reference docs
- `docs/specifications/SPEC_CATALOG_OBJECT_MODEL_V0.md`
- `docs/specifications/SPEC_DEFINITION_BATCH_V0.md`
- `docs/adr/ADR-0012-DEFINITION_BATCH_PUBLICATION.md`

## Guardrails
- No partially visible catalog mutation.
- No catalog truth kept only in RAM.
- No bypass around ProcedureContract compatibility checks.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
