---
name: srpl-anti-dynamic-sql
description: "Reject dynamic SQL/text construction, implicit names, SELECT *, shape-shifting returns, unbounded loops, and ambient NULLs in SRPL. Use during language review."
---

# SRPL Anti Dynamic SQL

## Purpose

Use this skill to execute a focused, reusable workflow for **SRPL Anti Dynamic SQL** in the Andromeda project.

This skill is intentionally not an agent. It does not own planning, delegation, or final authority. It provides domain-specific procedure, review points, and validation criteria that an agent can load when the user request requires this capability.

## Scope

Use this skill when the active task requires:

- Precise work in the `andromeda` domain.
- Alignment with Andromeda's strict relational, transactional, contract-first doctrine.
- Rust 2024-compatible engineering discipline when code is involved.
- Evidence-based validation instead of broad intuition.

## Required inputs

Collect or infer:

1. The target subsystem, file set, document set, or prompt.
2. The requested output format.
3. The risk class: experimental, important, critical, or mission-critical.
4. The source evidence that constrains the task.
5. The validation gate that proves the result.

## Procedure

1. Restate the task in one precise sentence.
2. Identify the applicable Andromeda invariants.
3. Identify the smallest safe scope that satisfies the request.
4. Apply the workflow from `references/checklist.md`.
5. Produce an output that separates:
   - findings,
   - decisions,
   - proposed changes,
   - validation,
   - residual risk.
6. For code or repository changes, name the exact commands that should be run.
7. For mission-critical claims, require a traceable source or an explicit uncertainty note.

## Andromeda guardrails

Always preserve these defaults:

- No application-facing ad hoc SQL.
- Procedure contracts are typed, versioned, and hashable.
- WAL durability precedes visible commit.
- Recovery and audit are design requirements, not afterthoughts.
- GPU and learned components may propose or accelerate, but they do not decide critical truth.
- Rust persistent/network formats use explicit codecs, never native struct layout.

## Validation

Use the strongest applicable validation:

- Documentation-only change: consistency check against Andromeda doctrine.
- Codex tooling change: `python3 .codex/scripts/validate_codex_tooling.py`.
- Rust workspace change: `cargo fmt`, `cargo check`, `cargo clippy`, and relevant tests.
- Parser/codec change: property tests and fuzz tests.
- WAL/storage/recovery change: crash/recovery matrix.
- Security/RPC change: permission, threat-model, and audit-trace review.

## Output contract

Return:

```text
Task
Sources used
Decision or change
Validation performed or required
Risks
Next executable step
```

## Reference

Read `references/checklist.md` when the task needs detailed checks or acceptance criteria.
