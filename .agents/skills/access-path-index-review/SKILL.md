---
name: access-path-index-review
description: "Review access path design for B+Tree, hash, hot/cold merge, and scan requirements. Use when working on storage and optimizer tasks for the Andromeda relational transactional database engine project."
---

# access-path-index-review

## Purpose

Review access path design for B+Tree, hash, hot/cold merge, and scan requirements.

## Use when

Use this skill for focused storage and optimizer work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Identify predicates.
2. Choose access path.
3. Check range support.
4. Check update cost.
5. Check recovery story.

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
