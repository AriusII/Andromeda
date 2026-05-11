---
name: architecture-workspace-governance
description: Applies Andromeda workspace governance when prompts mention architecture, crates, folders, tests, tooling, module ownership, or repository structure.
license: MIT
---

# architecture-workspace-governance

## When to use
- The user asks where code, tests, crates, docs, or tooling should live.
- A change creates, renames, splits, or connects crates or modules.
- A review concerns crate ownership, repository architecture, workspace dependency topology, or executable test placement.

## Purpose
Keep the repository boundary-heavy and ownership-driven. The skill guides agents to place code, tests, docs, and tooling in the correct crate or architecture layer without creating generic buckets or cross-cutting shortcuts.

## Process
1. Read repository and engine architecture before proposing file movement or new crates.
2. Identify the functional domain owner: catalog, SRPL, execution, transaction, storage, network, optimizer, operations, or contract/codec layer.
3. Keep lib.rs minimal: module declarations and intentional re-exports only; place implementation in owned modules.
4. Put executable runtime tests with the owning crate, not root tests, and preserve deterministic validation.
5. Check that new dependencies respect crate responsibility and do not create common/utils/misc/helper/god-engine buckets.

## Expected output
- A placement decision naming the owning crate, module, and test location.
- A dependency/topology risk list for any new coupling.
- Validation commands, especially workspace topology or crate-owned integration tests when relevant.

## Reference docs
- `docs/architecture/REPOSITORY_ARCHITECTURE.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/architecture/FOUR_ENGINE_MODEL.md`
- `crates/README.md`

## Guardrails
- Do not create generic shared buckets.
- Do not move runtime behavior tests to root tests/.
- Do not collapse contract crates into runtime crates for convenience.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
