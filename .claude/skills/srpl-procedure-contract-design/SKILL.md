---
name: srpl-procedure-contract-design
description: Design strict SRPL procedure signatures and contracts. Use when working on srpl tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Write
version: 0.1.0
---

# srpl-procedure-contract-design

## Purpose

Design strict SRPL procedure signatures and contracts.

## Use when

Use this skill for focused srpl work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Define inputs.
2. Define StructuredObjects.
3. Define result streams.
4. Define errors.
5. Define transaction policy.
6. Define ContractHash inputs.

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
