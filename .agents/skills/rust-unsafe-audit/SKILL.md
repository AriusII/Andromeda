---
name: rust-unsafe-audit
description: "Audit unsafe Rust plans or code for memory, aliasing, and lifetime invariants. Use when working on rust tasks for the Andromeda relational transactional database engine project."
---

# rust-unsafe-audit

## Purpose

Audit unsafe Rust plans or code for memory, aliasing, and lifetime invariants.

## Use when

Use this skill for focused rust work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Locate unsafe surface.
2. Identify invariant.
3. Check safe wrapper.
4. Check tests.
5. Check panic behavior.
6. Return risk rating.

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
