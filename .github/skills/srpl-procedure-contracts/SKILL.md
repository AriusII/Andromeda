---
name: srpl-procedure-contracts
description: Handles SRPL ProcedureContract, ContractHash, compatibility, and typed procedure semantics when prompts mention SRPL contracts, hashes, or procedure versions.
license: MIT
---

# srpl-procedure-contracts

## When to use
- The prompt mentions SRPL procedure contracts, ProcedureContract, ContractHash, compatibility, signature, parameter type, result type, or procedure versioning.
- A parser, binder, catalog, execution, or RPC change could affect the contract digest or procedure ABI.
- Tests fail around contract hash canonicalization or type-system compatibility.

## Purpose
Protect the procedure-only surface by ensuring SRPL changes produce stable, typed, versioned contracts with canonical hashes and explicit compatibility behavior. The skill keeps application execution tied to cataloged Procedure contracts rather than ad hoc runtime interpretation.

## Process
1. Read the SRPL type-system architecture and contract specification before editing code.
2. Identify every contract field affected by the change, including parameters, result shape, effects, policy binding, catalog version, and compatibility metadata.
3. Preserve canonical ContractHash behavior; update only through the documented canonicalization rules and add digest regression tests.
4. Check compatibility direction explicitly: additive compatible change, breaking change, or rejected ambiguity.
5. Ensure execution and RPC paths consume cataloged ProcedureContract data, not raw text or inferred local structs.

## Expected output
- A contract-impact summary with affected fields and compatibility classification.
- Required digest, binder, and catalog contract tests.
- A note on whether existing procedure versions must be migrated, rejected, or preserved.

## Reference docs
- `docs/architecture/SRPL_TYPE_SYSTEM_ARCHITECTURE.md`
- `docs/specifications/SPEC_PROCEDURE_CONTRACT_V0.md`
- `docs/specifications/SPEC_TYPE_SYSTEM_V0.md`
- `docs/adr/ADR-0011-CONTRACT_HASH_CANONICALIZATION.md`

## Guardrails
- Do not make native Rust struct layout part of the contract format.
- Do not allow text SQL/SRPL execution to bypass the cataloged contract.
- Do not change hashes without canonicalization evidence.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
