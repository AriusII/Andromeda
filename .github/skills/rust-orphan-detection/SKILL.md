---
name: rust-orphan-detection
description: Find orphan files modules crates features examples benches scripts or docs when orphan stale unowned or disconnected keywords appear.
license: MIT
---

# rust-orphan-detection

## When to use
Use for suspected unreferenced files, modules not declared, unused crate members, stale examples/benches/scripts, disconnected docs, or broad repository hygiene requests. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-orphan-detection` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Prove whether artifacts have an owner, entry point, manifest reference, documentation path, and validation path before removal or reassignment. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Search manifests, module declarations, CI/workflow references, docs indexes, and tests for each candidate.
2. Use `cargo metadata --locked` and code intelligence to distinguish workspace members from ignored folders.
3. Classify findings as remove, reconnect, move to owner, document, or leave intentionally external.
4. For many candidates, dispatch read-only `/agent` workers per artifact class and require one evidence table per worker.
5. Apply removals only after bounded confirmation and run `cargo check --workspace --locked` or relevant docs validation if docs are touched.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/architecture/REPOSITORY_ARCHITECTURE.md`
- `docs/README.md`
- `docs/testing/CI_GATES.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not delete files solely because grep misses dynamic references.
- Do not edit the docs tree unless the user explicitly asks outside this skill-writing context.
- Do not treat generated or future-proof specs as dead without owner confirmation.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
