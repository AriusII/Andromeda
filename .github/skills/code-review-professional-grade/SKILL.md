---
name: code-review-professional-grade
description: Perform high signal Andromeda Rust code review when review PR diff risk bug security or correctness keywords appear.
license: MIT
---

# code-review-professional-grade

## When to use
Use for reviewing diffs, PRs, staged changes, agent output, risky Rust changes, security/durability regressions, or requests for professional-grade review. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill code-review-professional-grade` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Find substantive bugs and risk, not style noise, using Andromeda invariants and executable evidence. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Inspect the diff first, then read surrounding code and specs for changed domains.
2. Prioritize correctness, safety, durability, security, concurrency, compatibility, and test gaps over formatting or preferences.
3. For each finding, cite file/line, explain impact, and suggest a concrete fix or validation.
4. Use `/agent` read-only reviewers for independent large areas, but consolidate to avoid duplicate or contradictory comments.
5. Run or request targeted validation only when it materially supports the review.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/project/ANDROMEDA_DOCTRINE.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/testing/RELEASE_GATES.md`
- `docs/adr/ADR-0005-WAL_DURABILITY_POLICY.md`
- `docs/README.md`

## Guardrails
- Never leave style-only nits.
- Do not speculate without evidence.
- Do not approve changes that allow visible commit before durable WAL or bypass Procedure/security admission.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
