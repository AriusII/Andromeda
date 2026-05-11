---
name: rust-error-modeling
description: Design typed Rust errors and protocol conversions when ErrKind thiserror anyhow error mapping or RPC error keywords appear.
license: MIT
---

# rust-error-modeling

## When to use
Use for stringly errors, `ErrKind`, domain error enums, `thiserror`, `anyhow`, protocol error mapping, audit-safe messages, and error conversion at RPC or storage boundaries. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-error-modeling` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Make failures machine-classifiable in libraries and safely convertible at external boundaries without leaking secrets or losing source chains. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Identify the owning domain and define a typed error enum with stable categories; reserve `anyhow` for CLI/tooling orchestration.
2. Preserve `source` chains and add context at boundaries, not by formatting strings deep in libraries.
3. Map domain errors to custom Protobuf RPC error/status families only at protocol edges.
4. Add tests for classification, conversion, redaction, and retryability where relevant.
5. Use `/agent` read-only review for cross-crate migrations, then edit in bounded slices and run `cargo check --workspace --locked`.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/architecture/QUIC_RPC_SECURITY_ARCHITECTURE.md`
- `docs/specifications/SPEC_RPC_FRAME_V0.md`
- `docs/specifications/SPEC_DECISION_TRACE_V0.md`
- `docs/adr/ADR-0007-QUIC_RPC_BOUNDARY_NO_GRPC.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- No string-only critical errors.
- No leaking credentials, paths, or security-sensitive details into client errors.
- No gRPC status model or JSON native protocol substitution.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
