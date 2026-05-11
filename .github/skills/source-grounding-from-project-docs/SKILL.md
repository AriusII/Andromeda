---
name: source-grounding-from-project-docs
description: Grounds Andromeda decisions in canonical project docs when prompts mention source basis, docs, terminology, status, readiness, or current state.
license: MIT
---

# source-grounding-from-project-docs

## When to use
- The user asks what is true, current, ready, accepted, canonical, or documented in Andromeda.
- A task depends on naming, terminology, documentation status, roadmap claims, or source-of-truth selection.
- There is a conflict between ADR history, roadmap intent, code shape, and docs/status.md.

## Purpose
Prevent stale roadmap lore, outdated ADR assumptions, or invented terminology from driving work. The skill makes the agent establish the authoritative source hierarchy, verify the current status, and map vocabulary before generating plans, edits, or claims.

## Process
1. Start with docs/INDEX.md and docs/README.md to identify the canonical navigation path.
2. Read docs/status.md before making readiness or implemented-state claims; treat historical roadmap and ADR text as context unless status confirms current state.
3. Use SOURCE_BASIS and TERMINOLOGY_MAPPING to translate legacy names into current Andromeda terms.
4. When citing project concepts, prefer exact glossary terms and avoid inventing synonyms for Procedure, ContractHash, CatalogVersion, ResultStream, WAL, or C5.
5. Return assumptions explicitly when the docs are silent, and separate code observations from documented guarantees.

## Expected output
- A source-grounded answer with exact relative doc citations.
- A current-state versus intended-future distinction when applicable.
- Terminology corrections and any unresolved documentation gaps.

## Reference docs
- `docs/INDEX.md`
- `docs/README.md`
- `docs/status.md`
- `docs/reference/SOURCE_BASIS.md`
- `docs/reference/TERMINOLOGY_MAPPING.md`
- `docs/project/GLOSSARY.md`

## Guardrails
- Never use roadmap phase text alone as proof of readiness.
- Do not edit docs while using this skill unless the user explicitly asks for doc changes and the task scope allows it.
- Prefer canonical names over local aliases.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
