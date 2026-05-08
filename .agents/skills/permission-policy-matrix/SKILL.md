---
name: permission-policy-matrix
description: "Build or review permission and policy matrices. Use when working on security tasks for the Andromeda relational transactional database engine project."
---

# permission-policy-matrix

## Purpose

Build or review permission and policy matrices.

## Use when

Use this skill for focused security work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. List actions.
2. List principals.
3. Define roles.
4. Define denials.
5. Define audit requirements.

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
