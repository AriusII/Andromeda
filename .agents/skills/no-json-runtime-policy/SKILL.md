---
name: no-json-runtime-policy
description: Scan runtime protocol artifacts for JSON default format drift. Use when working on doctrine and protocol tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob
version: 0.1.0
---

# no-json-runtime-policy

## Purpose

Scan runtime protocol artifacts for JSON default format drift.

## Use when

Use this skill for focused doctrine and protocol work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Search JSON usage.
2. Classify config vs runtime wire.
3. Flag unsafe runtime defaults.
4. Suggest Protobuf or binary metadata.

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
