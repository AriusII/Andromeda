---
name: definition-batch-dryrun
description: Design or review DefinitionBatch dry-run and apply behavior. Use when working on catalog tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Write
version: 0.1.0
---

# definition-batch-dryrun

## Purpose

Design or review DefinitionBatch dry-run and apply behavior.

## Use when

Use this skill for focused catalog work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. List operations.
2. Build dependency order.
3. Detect conflicts.
4. Define dry-run output.
5. Define rollback behavior.

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
