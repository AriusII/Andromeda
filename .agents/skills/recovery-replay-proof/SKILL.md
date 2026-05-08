---
name: recovery-replay-proof
description: "Prove or review recovery replay procedure. Use when working on recovery tasks for the Andromeda relational transactional database engine project."
---

# recovery-replay-proof

## Purpose

Prove or review recovery replay procedure.

## Use when

Use this skill for focused recovery work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Define starting manifest.
2. Define WAL range.
3. Define redo sequence.
4. Define incomplete transaction handling.
5. Define validation and open criteria.

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
