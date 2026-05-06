---
name: statistics-histogram-design
description: Design statistics, histograms, skew detection, and StatsVersion publication. Use when working on statistics tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Write
version: 0.1.0
---

# statistics-histogram-design

## Purpose

Design statistics, histograms, skew detection, and StatsVersion publication.

## Use when

Use this skill for focused statistics work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Identify columns.
2. Choose statistic type.
3. Define refresh policy.
4. Define validation.
5. Define publication switch.

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
