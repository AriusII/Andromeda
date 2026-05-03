---
name: wal-record-design
description: Design WAL record types and durability semantics. Use when working on transaction and storage tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Write
version: 0.1.0
---

# wal-record-design

## Purpose

Design WAL record types and durability semantics.

## Use when

Use this skill for focused transaction and storage work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Define record schema.
2. Define LSN behavior.
3. Define checksum/hash.
4. Define redo data.
5. Define incomplete transaction behavior.

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
