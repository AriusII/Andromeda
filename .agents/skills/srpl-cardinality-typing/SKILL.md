---
name: srpl-cardinality-typing
description: Review SRPL expressions for explicit type and cardinality semantics. Use when working on srpl tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob
version: 0.1.0
---

# srpl-cardinality-typing

## Purpose

Review SRPL expressions for explicit type and cardinality semantics.

## Use when

Use this skill for focused srpl work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Identify expressions.
2. Assign type and cardinality.
3. Check singleton assumptions.
4. Check optional handling.
5. Report unsafe ambiguity.

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
