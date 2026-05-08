---
name: performance-budgeting
description: "Define performance budgets and regression thresholds. Use when working on performance tasks for the Andromeda relational transactional database engine project."
---

# performance-budgeting

## Purpose

Define performance budgets and regression thresholds.

## Use when

Use this skill for focused performance work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Identify path.
2. Define budget.
3. Define metric.
4. Define hardware profile.
5. Define failure threshold.

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
