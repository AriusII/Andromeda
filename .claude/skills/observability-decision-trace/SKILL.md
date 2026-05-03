---
name: observability-decision-trace
description: Define DecisionTrace and observability fields for plan, recovery, and admin decisions. Use when working on observability tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Write
version: 0.1.0
---

# observability-decision-trace

## Purpose

Define DecisionTrace and observability fields for plan, recovery, and admin decisions.

## Use when

Use this skill for focused observability work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Identify decision.
2. Capture inputs.
3. Capture versions.
4. Capture alternatives.
5. Capture final reason.
6. Define queryability.

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
