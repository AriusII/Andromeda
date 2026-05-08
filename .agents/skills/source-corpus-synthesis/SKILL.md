---
name: source-corpus-synthesis
description: "Synthesize project and external research sources into decision-ready notes. Use when working on research and documentation tasks for the Andromeda relational transactional database engine project."
---

# source-corpus-synthesis

## Purpose

Synthesize project and external research sources into decision-ready notes.

## Use when

Use this skill for focused research and documentation work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. List sources.
2. Classify each source.
3. Extract stable claims.
4. Separate consensus from frontier.
5. Map safe claims to Andromeda.
6. State limitations.

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
