---
name: decision-record-authoring
description: Author decision records for durable architecture or policy changes. Use when working on governance tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Write
version: 0.1.0
---

# decision-record-authoring

## Purpose

Author decision records for durable architecture or policy changes.

## Use when

Use this skill for focused governance work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Define context.
2. State decision.
3. List alternatives.
4. State consequences.
5. Define validation.
6. Define rollback.

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
