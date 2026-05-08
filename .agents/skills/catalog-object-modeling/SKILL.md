---
name: catalog-object-modeling
description: "Model catalog objects, versions, dependencies, and contract hashes. Use when working on catalog tasks for the Andromeda relational transactional database engine project."
---

# catalog-object-modeling

## Purpose

Model catalog objects, versions, dependencies, and contract hashes.

## Use when

Use this skill for focused catalog work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Define object identity.
2. Define version fields.
3. Define dependency graph.
4. Define compatibility rules.
5. Define trace fields.

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
