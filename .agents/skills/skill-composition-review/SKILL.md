---
name: skill-composition-review
description: "Review whether skills are atomic and correctly composed by agents. Use when working on agent ops tasks for the Andromeda relational transactional database engine project."
---

# skill-composition-review

## Purpose

Review whether skills are atomic and correctly composed by agents.

## Use when

Use this skill for focused agent ops work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Find overlapping skills.
2. Check descriptions.
3. Check allowed tools.
4. Check duplication with agents.
5. Propose split/merge.

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
