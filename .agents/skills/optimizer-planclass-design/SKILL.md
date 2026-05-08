---
name: optimizer-planclass-design
description: "Design bounded multi-plan classes and plan cache behavior. Use when working on optimizer tasks for the Andromeda relational transactional database engine project."
---

# optimizer-planclass-design

## Purpose

Design bounded multi-plan classes and plan cache behavior.

## Use when

Use this skill for focused optimizer work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Identify parameter shapes.
2. Define PlanClass.
3. Define cache key.
4. Define eviction.
5. Define hysteresis.

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
