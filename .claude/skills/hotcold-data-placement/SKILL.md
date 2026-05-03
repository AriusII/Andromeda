---
name: hotcold-data-placement
description: Classify data placement across RAM, HotStore NVMe, and ColdStore HDD. Use when working on storage tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Write
version: 0.1.0
---

# hotcold-data-placement

## Purpose

Classify data placement across RAM, HotStore NVMe, and ColdStore HDD.

## Use when

Use this skill for focused storage work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Classify data temperature.
2. Assign storage tier.
3. Define write path.
4. Define read fallback.
5. Define metrics.

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
