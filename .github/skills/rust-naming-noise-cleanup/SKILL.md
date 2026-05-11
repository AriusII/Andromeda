---
name: rust-naming-noise-cleanup
description: Clean Rust naming noise when rename terminology casing module file function or test name keywords appear.
license: MIT
---

# rust-naming-noise-cleanup

## When to use
Use for inconsistent names, vague identifiers, noisy suffixes, test names, file/module naming, terminology alignment, and safe rename requests. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-naming-noise-cleanup` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Make names precise, domain-aligned, and searchable without semantic drift. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Use rename refactoring/code intelligence for symbols instead of textual replacement.
2. Check terminology against glossary/spec names before inventing new terms.
3. Rename in small groups: files/modules, public symbols, private helpers, tests, and fixtures separately.
4. Preserve public compatibility or update all downstream uses in the same slice.
5. Validate with `cargo check --workspace --locked`, targeted tests, and grep for old names; use `/agent` for read-only terminology inventories.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/project/GLOSSARY.md`
- `docs/reference/TERMINOLOGY_MAPPING.md`
- `docs/architecture/REPOSITORY_ARCHITECTURE.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not churn names for personal preference.
- Do not rename persisted fields or protocol variants without explicit migration work.
- Do not obscure Andromeda terms like Procedure, SRPL, WAL, ResultStream, or contract.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
