---
name: prompt-injection-threat-model
description: "Threat-model prompt injection and agentic manipulation. Use when working on security tasks for the Andromeda relational transactional database engine project."
---

# prompt-injection-threat-model

## Purpose

Threat-model prompt injection and agentic manipulation.

## Use when

Use this skill for focused security work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Identify untrusted inputs.
2. Define attacker objective.
3. Define tool risk.
4. Define guardrail.
5. Define human confirmation.

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
