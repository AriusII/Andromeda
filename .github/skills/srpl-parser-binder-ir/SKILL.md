---
name: srpl-parser-binder-ir
description: Guides SRPL parser, binder, semantic IR, ALT, and plan preparation work when prompts mention grammar, binding, semantic IR, lowering, or plan prep.
license: MIT
---

# srpl-parser-binder-ir

## When to use
- The user mentions SRPL grammar, parser, lexer, binder, semantic IR, ALT, lowering, diagnostics, or plan preparation.
- A change touches andromeda-srpl-* crates or execution adapter boundaries.
- A bug involves name resolution, type inference, semantic diagnostics, or lowered plan input.

## Purpose
Keep SRPL language processing deterministic, typed, and contract-aware from grammar through binder and semantic IR. The skill prevents parser convenience from leaking unresolved symbols, ambiguous types, or execution-only assumptions into durable contracts or plans.

## Process
1. Start from the grammar, binder, and semantic IR specs and identify which phase owns the behavior.
2. Keep syntax, binding, semantic validation, and lowering responsibilities separate; do not fix binder bugs in runtime execution.
3. Ensure diagnostics are deterministic and source-located while internal IR remains typed and explicit.
4. Verify catalog access is versioned and does not introduce direct storage or transaction side effects.
5. Add or update grammar, binder, IR, and plan-prep tests in the owning SRPL crate.

## Expected output
- A phase ownership map for parser, binder, IR, and plan prep.
- Concrete acceptance cases for valid syntax, invalid syntax, invalid binding, and semantic lowering.
- The specific crate-owned tests needed to prove behavior.

## Reference docs
- `docs/specifications/SPEC_SRPL_GRAMMAR_V0.md`
- `docs/specifications/SPEC_SRPL_BINDER_V0.md`
- `docs/specifications/SPEC_SEMANTIC_IR_V0.md`

## Guardrails
- Do not let unbound names or inferred ad hoc shapes reach execution.
- Do not make parser text the durable or wire contract.
- Do not bypass ProcedureContract generation.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
