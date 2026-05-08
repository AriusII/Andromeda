---
name: agent-handoff-contracting
description: "Write precise handoff contracts between agents. Use when working on agent ops tasks for the Andromeda relational transactional database engine project."
---

# agent-handoff-contracting

## Purpose

Write precise handoff contracts between agents.

## Use when

Use this skill for focused agent ops work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Summarize objective.
2. List evidence.
3. List assumptions.
4. List decisions.
5. Define requested output.

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
