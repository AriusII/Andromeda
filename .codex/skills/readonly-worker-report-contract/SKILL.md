---
name: readonly-worker-report-contract
description: "Use by repository-read-only workers that must inspect code and produce one detailed Markdown report under .work."
category: codex-workers
---

# readonly-worker-report-contract

## When to use
A worker is assigned read-only analysis or audit.

## Purpose
Use by repository-read-only workers that must inspect code and produce one detailed Markdown report under .work.

## Process
- Inspect only assigned paths and evidence.
- Do not mutate repository files; only create one `.work` report.
- Include inspected paths, commands, evidence, findings, TODO/SUB-TODO, dependencies, proposed write workers, and validation gates.
- Use severity levels: Critical, High, Medium, Low, Advisory.

## Expected output
- Exactly one Markdown report under `.work/codex/<task-slug>/analysis/`.

## Guardrails
- No source edits. No multiple report files. No claims without evidence.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
