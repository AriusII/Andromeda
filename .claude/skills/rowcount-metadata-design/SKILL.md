---
name: rowcount-metadata-design
description: Specify exact row count metadata for procedure results and StructuredObjects. Use when working on protocol tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Write
version: 0.1.0
---

# rowcount-metadata-design

## Purpose

Specify exact row count metadata for procedure results and StructuredObjects.

## Use when

Use this skill for focused protocol work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Determine exactness policy.
2. Define pre-payload metadata.
3. Define streaming effect.
4. Define failure behavior.
5. Define client allocation hints.

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
