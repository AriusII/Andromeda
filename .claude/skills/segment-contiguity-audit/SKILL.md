---
name: segment-contiguity-audit
description: Audit segment and page layouts for cold contiguity invariants. Use when working on storage tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob
version: 0.1.0
---

# segment-contiguity-audit

## Purpose

Audit segment and page layouts for cold contiguity invariants.

## Use when

Use this skill for focused storage work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Check append-only rule.
2. Check no split after cold write.
3. Check immutable publication.
4. Check manifest switch.
5. Report fragmentation risks.

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
