---
name: scientific-crosscheck
description: "Cross-check technical claims against source classes and project doctrine. Use when working on research tasks for the Andromeda relational transactional database engine project."
---

# scientific-crosscheck

## Purpose

Cross-check technical claims against source classes and project doctrine.

## Use when

Use this skill for focused research work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Identify claim.
2. Find source.
3. Classify evidence.
4. Check conflicts.
5. State uncertainty.

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
