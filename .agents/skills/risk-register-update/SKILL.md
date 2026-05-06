---
name: risk-register-update
description: Update risks with severity, likelihood, mitigation, owner, and status. Use when working on governance tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Write, Edit
version: 0.1.0
---

# risk-register-update

## Purpose

Update risks with severity, likelihood, mitigation, owner, and status.

## Use when

Use this skill for focused governance work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Identify risk.
2. Classify severity.
3. Define mitigation.
4. Assign owner.
5. Define review trigger.

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
