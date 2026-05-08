---
name: map-refresh-policy
description: "Define Map consistency and refresh mode. Use when working on maps tasks for the Andromeda relational transactional database engine project."
---

# map-refresh-policy

## Purpose

Define Map consistency and refresh mode.

## Use when

Use this skill for focused maps work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Classify Map.
2. Estimate maintenance cost.
3. Choose refresh mode.
4. Define transaction interaction.
5. Define stale-read policy.

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
