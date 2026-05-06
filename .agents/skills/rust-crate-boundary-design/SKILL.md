---
name: rust-crate-boundary-design
description: Design Rust crate boundaries aligned with engine modules. Use when working on rust tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Write
version: 0.1.0
---

# rust-crate-boundary-design

## Purpose

Design Rust crate boundaries aligned with engine modules.

## Use when

Use this skill for focused rust work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. List engine domains.
2. Define crate ownership.
3. Check dependency direction.
4. Define feature flags.
5. Define public API boundaries.

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
