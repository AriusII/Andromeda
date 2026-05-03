---
name: microsoft-doc-style-edit
description: Edit documentation to Microsoft documentation style. Use when working on documentation tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Edit
version: 0.1.0
---

# microsoft-doc-style-edit

## Purpose

Edit documentation to Microsoft documentation style.

## Use when

Use this skill for focused documentation work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Check headings.
2. Shorten paragraphs.
3. Normalize terminology.
4. Convert rambling text to procedures.
5. Add tables where useful.

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
