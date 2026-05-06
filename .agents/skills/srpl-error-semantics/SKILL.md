---
name: srpl-error-semantics
description: Define SRPL failure, rollback, poison, and error return behavior. Use when working on srpl tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Write
version: 0.1.0
---

# srpl-error-semantics

## Purpose

Define SRPL failure, rollback, poison, and error return behavior.

## Use when

Use this skill for focused srpl work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. List failure cases.
2. Map to transaction states.
3. Define rollback behavior.
4. Define returned error contract.
5. Define audit trace fields.

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
