---
name: project-invariant-check
description: Check an artifact against Andromeda non-negotiable invariants. Use when working on doctrine tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob
version: 0.1.0
---

# project-invariant-check

## Purpose

Check an artifact against Andromeda non-negotiable invariants.

## Use when

Use this skill for focused doctrine work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Identify artifact scope.
2. Compare against invariants.
3. Flag violations.
4. Classify severity.
5. Suggest safe correction.
6. Return pass/fail.

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
