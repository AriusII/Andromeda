---
name: maps-analytics-summarizability
description: "Use for Maps, analytical projections, grain, refresh policy and summarizability."
category: andromeda-analytics
---

# maps-analytics-summarizability

## When to use
Maps or analytics are involved.

## Purpose
Use for Maps, analytical projections, grain, refresh policy and summarizability.

## Process
- Declare grain and summarizability constraints.
- Choose refresh mode: Immediate, Incremental, Deferred, SnapshotOnly.
- Keep wide analytical maps off OLTP commit path.
- Use GPU only for batch outside commit/recovery.

## Expected output
- Map design/review.

## Guardrails
- No virtual View as native generic object.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
