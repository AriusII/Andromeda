---
name: procedure-store-evidence
description: Manages Procedure Store, ScenarioEvidence, and feedback loops when prompts mention procedure store, evidence, learned components, scenarios, or feedback.
license: MIT
---

# procedure-store-evidence

## When to use
- The prompt mentions Procedure Store, ScenarioEvidence, learned component, feedback loop, recommendation, evidence, or adaptive optimization.
- A change touches procedure-store, scenario-evidence, optimizer feedback, maps analytics, or decision traces.
- A review asks whether machine-learned behavior can affect correctness.

## Purpose
Keep scenario evidence and learned/advisory feedback as auditable inputs rather than hidden authority. The skill ensures procedure-store feedback loops are versioned, explainable, disableable, and never placed on commit, recovery, or security-critical paths.

## Process
1. Read the ScenarioEvidence spec and evidence/learned-component ADRs.
2. Classify data as evidence, advisory decision input, or correctness authority; only the first two are allowed for learned components.
3. Attach version, provenance, scope, disable switch, and traceability to any feedback loop.
4. Ensure fallback behavior works without the evidence and never blocks commit, recovery, MVCC visibility, or authorization.
5. Add tests for evidence ingestion, trace emission, disablement, stale evidence rejection, and fallback behavior.

## Expected output
- An evidence boundary statement showing what the feedback may and may not influence.
- ScenarioEvidence fields and provenance requirements.
- Fallback, disablement, and DecisionTrace validation.

## Reference docs
- `docs/specifications/SPEC_SCENARIO_EVIDENCE_V0.md`
- `docs/adr/ADR-0013-PROCEDURE_STORE_EVIDENCE.md`
- `docs/adr/ADR-0018-LEARNED_COMPONENTS_AS_EVIDENCE.md`

## Guardrails
- No learned component as correctness authority.
- No evidence path in commit/recovery/security-critical code.
- No untraceable adaptive decisions.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
