---
name: write-worker-mission-report-contract
description: Define Copilot write worker mission reports when bounded edit worker implementation or mission keywords appear.
license: MIT
---

# write-worker-mission-report-contract

## When to use
Use for `/agent` write workers assigned a bounded code change, refactor, cleanup, or test addition with non-overlapping files. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill write-worker-mission-report-contract` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Ensure write workers edit safely and leave a durable mission record for consolidation. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Provide exact objective, allowed files/directories, forbidden areas, reference docs, skill hints, and validation commands.
2. Require the worker to make only the bounded edit and then write exactly one report to `.work/copilot-cli/<task-slug>/missions/<worker>.md`.
3. Mission report sections should include Scope, Files changed, Rationale, Tests/commands run, Results, Remaining risks, and Handoff notes.
4. Never schedule two write workers on overlapping files; use the SQL dependency queue to serialize conflicts.
5. The orchestrator must review diffs and rerun or spot-check validation before final consolidation.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/roadmap/EXECUTION_METHOD.md`
- `docs/roadmap/queues/DEPENDENCY_QUEUES.md`
- `docs/testing/CI_GATES.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- No unbounded rewrites.
- No edits outside the allowed path list.
- No success claim without validation evidence or explicit blocker.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
