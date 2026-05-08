---
name: predictive-evidence-scoring
description: "Define ScenarioEvidence scoring and optimizer consumption rules. Use when working on benchmark and optimizer tasks for the Andromeda relational transactional database engine project."
---

# predictive-evidence-scoring

## Purpose

Define ScenarioEvidence scoring and optimizer consumption rules.

## Use when

Use this skill for focused benchmark and optimizer work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Define scenario.
2. Define scores.
3. Separate confidence factors.
4. Define expiration.
5. Define optimizer boundary.

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
