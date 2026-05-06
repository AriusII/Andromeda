---
name: structuredobject-layout
description: Design StructuredObject row, column, or hybrid layouts. Use when working on protocol and type system tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Write
version: 0.1.0
---

# structuredobject-layout

## Purpose

Design StructuredObject row, column, or hybrid layouts.

## Use when

Use this skill for focused protocol and type system work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Define shape.
2. Define invariants.
3. Choose layout.
4. Define batch header.
5. Define validation rules.
6. Define compatibility rules.

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
