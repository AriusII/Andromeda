---
name: security-mtls-iam-review
description: "Review mTLS, certificate identity, UserPrincipal, and permissions. Use when working on security tasks for the Andromeda relational transactional database engine project."
---

# security-mtls-iam-review

## Purpose

Review mTLS, certificate identity, UserPrincipal, and permissions.

## Use when

Use this skill for focused security work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Identify identity chain.
2. Check certificate/user separation.
3. Check scope.
4. Check revocation.
5. Check audit.

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
