---
name: crash-recovery-test-design
description: Design crash and recovery test scenarios. Use when working on testing tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Write
version: 0.1.0
---

# crash-recovery-test-design

## Purpose

Design crash and recovery test scenarios.

## Use when

Use this skill for focused testing work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Identify crash points.
2. Define durable state.
3. Define expected replay.
4. Define validation.
5. Define forensic output.

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
