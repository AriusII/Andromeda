---
name: rust-clean-code-refactor
description: Perform professional Rust cleanup without behavior drift when refactor cleanup simplify or readability keywords appear.
license: MIT
---

# rust-clean-code-refactor

## When to use
Use for bounded cleanup, readability, simplification, small refactors, clippy-driven improvement, or comments asking for professional code quality without feature changes. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-clean-code-refactor` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Improve clarity while preserving behavior, tests, and Andromeda domain boundaries. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. State the bounded target and observable behavior that must not change before editing.
2. Use code intelligence for call sites, types, and references; avoid blind text replacement for symbols.
3. Make small reversible edits: simplify control flow, remove duplication, tighten visibility, rename via refactoring tools, and keep comments only where clarifying invariants.
4. Run format, clippy/check, and owning tests; if baseline fails, separate pre-existing failures from introduced ones.
5. For broad cleanup, use `/agent` read-only scouts and a todo/dependency queue before write workers.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/architecture/REPOSITORY_ARCHITECTURE.md`
- `docs/testing/TEST_STRATEGY.md`
- `docs/reference/DOCUMENTATION_STANDARD.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not rewrite algorithms casually.
- Do not mix cleanup with protocol or persistence format changes.
- Do not silence warnings with allows unless justified and local.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
