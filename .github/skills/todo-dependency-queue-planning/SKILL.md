---
name: todo-dependency-queue-planning
description: Represent TODO sub TODO and dependency queues when planning worker order or multi-step execution keywords appear.
license: MIT
---

# todo-dependency-queue-planning

## When to use
Use for multi-step tasks, dependencies, worker scheduling, TODO queues, SUB-TODO decomposition, blockers, validation ordering, or when several edits must not collide. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill todo-dependency-queue-planning` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Make execution order explicit so Copilot can progress autonomously while respecting dependencies and validation gates. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Create stable kebab-case TODO IDs in `sql` with clear descriptions and statuses.
2. Add dependency edges for prerequisites, shared-file conflicts, validation-after-write, and analysis-before-edit relationships.
3. Select ready work with no pending dependencies; mark `in_progress` before starting and `done` only after validation or evidence is complete.
4. For `/agent` workers, assign one queue item per mission and require output that can update the queue.
5. At consolidation, list completed, blocked, and deferred items with reasons.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/roadmap/queues/DEPENDENCY_QUEUES.md`
- `docs/roadmap/queues/CRITICAL_PATH.md`
- `docs/roadmap/EXECUTION_METHOD.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not use vague IDs like t1.
- Do not run dependent write tasks in parallel.
- Do not mark work done because code was written; mark it done after validation.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
