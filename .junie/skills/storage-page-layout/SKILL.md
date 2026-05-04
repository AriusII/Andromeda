---
name: storage-page-layout
description: Design page header, payload, slot directory, and trailer layouts. Use when working on storage tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Write
version: 0.1.0
---

# storage-page-layout

## Purpose

Design page header, payload, slot directory, and trailer layouts.

## Use when

Use this skill for focused storage work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Define page size.
2. Define header fields.
3. Define payload layout.
4. Define free-space rules.
5. Define trailer checks.
6. Define versioning.

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
