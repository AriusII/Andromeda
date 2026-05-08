---
name: storage-page-layout
description: "Design page header, payload, slot directory, and trailer layouts. Use when working on storage tasks for the Andromeda relational transactional database engine project."
---

# storage-page-layout

## Purpose

Design page header, payload, slot directory, and trailer layouts.

## Policy (DEC-032)

- **Page Size:** 16 KiB or 32 KiB (Candidate V1).
- **Endianness:** Big-endian for keys (order-preserving); mixed LE/BE for headers.
- **Verification:** Every page MUST start with a `PageHeader` (containing `LSN`, `Checksum`, `MagicNumber`) and MUST end with a `PageTrailer` (repeating the `Checksum`).

## Use when

Use this skill for focused storage work. Do not use it as a general architecture agent.

## Inputs

- Task brief.
- Relevant project files.
- Affected engine or plane.
- Current assumptions.
- Required output format.

## Procedure

1. Define page size.
2. Define header fields.
3. Define payload layout.
4. Define free-space rules.
5. Define trailer checks.
6. Define versioning.

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

- 0.2.0 (2026-05-05): Updated with DEC-032 (V1 Candidate format: 16/32 KiB, BE keys, header/trailer checksums).
- 0.1.0 (2026-05-03): Initial project-specific skill.
