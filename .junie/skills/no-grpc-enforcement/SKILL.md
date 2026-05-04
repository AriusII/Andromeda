---
name: no-grpc-enforcement
description: Scan artifacts for accidental gRPC introduction. Use when working on doctrine and protocol tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob
version: 0.1.0
---

# no-grpc-enforcement

## Purpose

Scan artifacts for accidental gRPC introduction.

## Use when

Use this skill for focused doctrine and protocol work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Search for gRPC terms.
2. Classify legitimate comparison vs design use.
3. Flag violations.
4. Suggest QUIC + Protobuf wording.

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
