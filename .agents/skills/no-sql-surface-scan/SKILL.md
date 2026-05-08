---
name: no-sql-surface-scan
description: "Scan artifacts for accidental ad hoc SQL surface introduction. Use when working on doctrine tasks for the Andromeda relational transactional database engine project."
---

# no-sql-surface-scan

## Purpose

Scan artifacts for accidental ad hoc SQL surface introduction.

## Use when

Use this skill for focused doctrine work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Search for SQL surface terms.
2. Classify pedagogical vs native use.
3. Flag violations.
4. Suggest SRPL/RPC alternative.

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
