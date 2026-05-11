---
name: rust-file-splitting-placement
description: Shrink and split oversized Rust files by owner and responsibility when large file module split placement or ownership keywords appear.
license: MIT
---

# rust-file-splitting-placement

## When to use
Use when a Rust file is too large, mixes responsibilities, has unrelated test fixtures, or a user says split, shrink, extract module, file placement, near owner, or organize. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-file-splitting-placement` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Turn large files into small owner-adjacent modules without changing runtime behavior or public API shape unless explicitly requested. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Use symbol search and file outline first; identify independent responsibilities, public items, tests, and private helpers.
2. Choose modules under the existing owner, not a generic shared location; keep `lib.rs` to declarations and intentional re-exports.
3. Move one coherent slice at a time and preserve visibility with `pub(crate)` before making anything public.
4. After each slice, run `cargo fmt --all -- --check` and the owning crate tests; use `/agent` only for read-only inventory on many files.
5. Document the final module map in the response, not in repository docs unless separately requested.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/architecture/REPOSITORY_ARCHITECTURE.md`
- `docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md`
- `docs/testing/TEST_STRATEGY.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not create `mod.rs` tangles that obscure ownership.
- Do not widen visibility to make moves compile unless the boundary really requires it.
- Do not combine cleanup with semantic rewrites.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
