---
name: agent-output-validation
description: Validate final agent outputs against task completion and project gates. Use when working on agent ops tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob
version: 0.1.0
---

# agent-output-validation

## Purpose

Validate final agent outputs against task completion and project gates.

## Use when

Use this skill for focused agent ops work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Check requested deliverable.
2. Check invariants.
3. Check citations/evidence.
4. Check tests.
5. Check open issues.

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
