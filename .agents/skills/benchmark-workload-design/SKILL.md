---
name: benchmark-workload-design
description: Design bounded benchmark workloads and evidence scenarios. Use when working on benchmark tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Write
version: 0.1.0
---

# benchmark-workload-design

## Purpose

Design bounded benchmark workloads and evidence scenarios.

## Use when

Use this skill for focused benchmark work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Define workload intent.
2. Define data shape.
3. Define parameters.
4. Define hardware profile.
5. Define stop rules.
6. Define regression metrics.

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
