---
name: rust-ci-quality-gates
description: Run Andromeda Rust CI and release quality gates when CI clippy fmt test nextest release gate or validation keywords appear.
license: MIT
---

# rust-ci-quality-gates

## When to use
Use for validation plans, pre-PR checks, CI failures, release gates, nextest profiles, clippy warnings, formatting, or any completed Rust code change. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-ci-quality-gates` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Select and run the right commands so the result is evidence, not a claim. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Always consider the canonical four: `cargo fmt --all -- --check`, `cargo check --workspace --locked`, `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`, `cargo test --workspace --all-features --locked`.
2. Use nextest profiles when runtime class matters: `cargo nextest run --profile default`, `ci`, `slow`, `recovery`, and `wal`.
3. For narrow edits, start with the owning crate/test command, then escalate to workspace gates as time allows.
4. Capture failures with enough tail output to diagnose; distinguish pre-existing baseline from introduced failures.
5. When using `/agent` validation workers, give exact commands and require them to return pass/fail plus log path or salient errors.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/testing/CI_GATES.md`
- `docs/testing/RELEASE_GATES.md`
- `docs/testing/TEST_STRATEGY.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not mark a gate passed unless it ran successfully.
- Do not skip lockfile validation for dependency changes.
- Do not replace Rust gates with docs-only checks for code changes.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
