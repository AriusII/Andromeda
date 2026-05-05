---
name: hadr-quorum-review
description: Review HA/DR quorum, fencing, failover, and promotion rules. Use when working on ha/dr tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Write
version: 0.2.0
---

# hadr-quorum-review

## Purpose

Review HA/DR quorum, fencing, failover, and promotion rules.

## Policy (DEC-020)

- **Quorum:** Majority quorum MUST be maintained for leader promotion and synchronous durability.
- **Epoch:** `MembershipEpoch` MUST be tracked to prevent split-brain.
- **Election:** Leader election uses LSN-based ranking; only nodes with the highest durable LSN in the quorum are eligible for promotion.

## Use when

Use this skill for focused ha/dr work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Define node roles.
2. Define quorum rule.
3. Define fencing.
4. Define promotion sequence.
5. Define divergence handling.

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

- 0.2.0 (2026-05-05): Updated with DEC-020 (Majority quorum, MembershipEpoch, LSN ranking).
- 0.1.0 (2026-05-03): Initial project-specific skill.
