---
name: protobuf-schema-review
description: Review Protobuf schemas for Andromeda contract compatibility. Use when working on protocol tasks for the Andromeda SGBDRT project.
allowed-tools: Read, Grep, Glob, Edit
version: 0.2.0
---

# protobuf-schema-review

## Purpose

Review Protobuf schemas for Andromeda contract compatibility.

## Policy (DEC-021)

- **Versioning:** Every frame envelope MUST carry `ProtocolVersion`. Current locked version: **V1.0**.
- **Metadata:** Metadata MUST precede payload. Result streams MUST declare shapes and `row_count_exact` before batches.
- **Tooling:** Use `prost` 0.14+ (Protobuf Edition 2024).

## Use when

Use this skill for focused protocol work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Check package and version.
2. Check field numbering.
3. Check reserved fields.
4. Check explicit optionality.
5. Check ResultStream metadata.
6. Reject gRPC service assumptions.

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

- 0.2.0 (2026-05-05): Updated with DEC-021 (Protocol V1.0, Metadata-Before-Payload, prost 0.14+).
- 0.1.0 (2026-05-03): Initial project-specific skill.
