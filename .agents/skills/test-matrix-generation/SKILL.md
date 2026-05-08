---
name: test-matrix-generation
description: "Generate test matrices for specs, code, protocols, and recovery scenarios. Use when working on testing tasks for the Andromeda relational transactional database engine project."
---

# test-matrix-generation

## Purpose

Generate test matrices for specs, code, protocols, and recovery scenarios.

## Use when

Use this skill for focused testing work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. List requirements.
2. Map tests.
3. Define positive cases.
4. Define negative cases.
5. Define crash/fuzz cases.
6. Define acceptance.

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
