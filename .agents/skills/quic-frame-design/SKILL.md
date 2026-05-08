---
name: quic-frame-design
description: "Design QUIC frame mapping for Andromeda RPC. Use when working on protocol tasks for the Andromeda relational transactional database engine project."
---

# quic-frame-design

## Purpose

Design QUIC frame mapping for Andromeda RPC.

## Use when

Use this skill for focused protocol work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Define frame type.
2. Define stream ownership.
3. Define metadata order.
4. Define error handling.
5. Define backpressure interaction.

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
