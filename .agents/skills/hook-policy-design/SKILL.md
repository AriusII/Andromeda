---
name: hook-policy-design
description: "Design hook policies for prompt, tool, artifact, and stop gates. Use when working on agent ops tasks for the Andromeda relational transactional database engine project."
---

# hook-policy-design

## Purpose

Design hook policies for prompt, tool, artifact, and stop gates.

## Use when

Use this skill for focused agent ops work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Identify lifecycle event.
2. Define matcher.
3. Define block/warn behavior.
4. Define script input.
5. Define audit record.

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
