---
name: mvcc-visibility-proof
description: "Review MVCC visibility rules for deterministic snapshot behavior. Use when working on transaction tasks for the Andromeda relational transactional database engine project."
---

# mvcc-visibility-proof

## Purpose

Review MVCC visibility rules for deterministic snapshot behavior.

## Use when

Use this skill for focused transaction work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Define timestamp model.
2. Define row header fields.
3. Evaluate visibility predicate.
4. Check GC preconditions.
5. Define anomalies prevented.

## Output contract

Return:

- `summary`
- `findings`
- `recommended_action`
- `risks`
- `required_tests`
- `open_questions`

## Boundaries

- Do not override project doctrine.
- Do not introduce gRPC.
- Do not introduce ad hoc SQL as a native application surface.
- Do not infer runtime behavior that is not specified.
- Escalate cross-domain decisions to the responsible agent.

## Version history

- 0.1.0 (2026-05-03): Initial project-specific skill.
