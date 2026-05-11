---
name: task-scope-bounding
description: Convert broad requests into precise bounded Copilot work orders when scope vague broad cleanup or roadmap keywords appear.
license: MIT
---

# task-scope-bounding

## When to use
Use when a request is too broad, ambiguous, high-risk, multi-crate, or says improve, clean up, implement roadmap, harden, audit, or refactor without a clear boundary. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill task-scope-bounding` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Prevent wandering by turning intent into concrete deliverables, exclusions, files/owners, validation, and stop conditions. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Restate the user's goal as one or more bounded work orders with assumptions instead of asking in non-interactive mode.
2. Identify domain owner, risk class, affected crates/docs, explicit non-goals, and expected output.
3. Choose direct work for small scopes, `/agent` read-only analysis for uncertain scopes, and write workers only after dependencies are clear.
4. Record TODO/SUB-TODO dependencies in `sql` when there are multiple steps.
5. Validate that each work order has a completion test: command, diff review, report, or evidence table.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/roadmap/EXECUTION_METHOD.md`
- `docs/roadmap/queues/DEPENDENCY_QUEUES.md`
- `docs/project/FEATURE_ACCEPTANCE_GATE.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not reinterpret a narrow request as a broad campaign.
- Do not edit before the boundary is known.
- Do not include docs or generated artifacts outside the requested scope.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
