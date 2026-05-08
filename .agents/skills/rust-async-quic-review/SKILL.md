---
name: rust-async-quic-review
description: "Review async Rust and QUIC code or design for cancellation, backpressure, and lifetimes. Use when working on rust and network tasks for the Andromeda relational transactional database engine project."
---

# rust-async-quic-review

## Purpose

Review async Rust and QUIC code or design for cancellation, backpressure, and lifetimes.

## Use when

Use this skill for focused rust and network work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Identify async boundaries.
2. Check cancellation semantics.
3. Check stream backpressure.
4. Check buffer ownership.
5. Check shutdown behavior.

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
