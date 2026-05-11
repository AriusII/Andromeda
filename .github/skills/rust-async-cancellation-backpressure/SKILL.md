---
name: rust-async-cancellation-backpressure
description: Design Rust async cancellation shutdown and backpressure when tokio async channel timeout or slow client keywords appear.
license: MIT
---

# rust-async-cancellation-backpressure

## When to use
Use for async runtimes, cancellation safety, graceful shutdown, bounded channels, slow clients, streaming, QUIC tasks, timeouts, and backpressure design. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-async-cancellation-backpressure` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Make asynchronous behavior deterministic, bounded, observable, and safe under cancellation and overload. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Map task ownership, lifetimes, cancellation points, channels, buffers, and shutdown signals before editing.
2. Prefer bounded queues, explicit permits, cancellation-safe select loops, and drop-aware cleanup over detached tasks.
3. Test slow consumer, disconnect, timeout, shutdown, and overload cases with deterministic time where existing tools support it.
4. For network/result stream changes, validate ordering and protocol errors against specs.
5. Use `/agent` read-only workers to trace independent async flows, then implement one flow at a time.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/architecture/QUIC_RPC_SECURITY_ARCHITECTURE.md`
- `docs/runbooks/RUNBOOK_SLOW_CLIENT_BACKPRESSURE.md`
- `docs/specifications/SPEC_RESULT_STREAM_V0.md`
- `docs/specifications/SPEC_RPC_FRAME_V0.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not use unbounded channels on request/commit paths without explicit justification.
- Do not leak tasks after cancellation.
- Do not bypass typed Procedure or security admission paths for convenience.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
