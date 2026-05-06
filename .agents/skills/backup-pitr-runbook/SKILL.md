---
name: backup-pitr-runbook
description: Write or review backup and point-in-time recovery runbooks. Use when working on backup tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Write
version: 0.1.0
---

# backup-pitr-runbook

## Purpose

Write or review backup and point-in-time recovery runbooks.

## Use when

Use this skill for focused backup work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Choose snapshot.
2. Choose WAL range.
3. Define restore target.
4. Define validation.
5. Define production opening criteria.

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
