---
name: copilot-orchestration-protocol
description: Coordinate Copilot CLI analysis planning writing validation and consolidation when broad multi-agent or orchestration keywords appear.
license: MIT
---

# copilot-orchestration-protocol

## When to use
Use `/skill copilot-orchestration-protocol` for broad refactors, roadmap execution, multi-step implementation, many-file cleanup, worker orchestration, or requests mentioning orchestrator, workers, analysis, plan, validation, or consolidation. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill copilot-orchestration-protocol` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Make Copilot CLI act as a disciplined orchestrator: bound the scope, dispatch only useful workers, avoid edit collisions, validate, and consolidate results. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Normalize the request into a bounded mission, assumptions, exclusions, and task slug.
2. Use available tools deliberately: code intelligence/rust-lsp for Rust understanding, `grep`/`glob`/`view` for search/read, `powershell` for cargo commands, `sql` for queues, and `/agent <name>` or the agent tool for independent workers.
3. Sequence phases: read-only analysis, dependency-aware plan, write missions, validation, final consolidation.
4. Give each worker complete context, exact files or directories, skill hints such as `/skill rust-ci-quality-gates`, allowed scope, and output path under `.work/copilot-cli/<task-slug>/` when reports are needed.
5. Before final response, merge worker reports, verify commands, and state unresolved blockers.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/roadmap/EXECUTION_METHOD.md`
- `docs/roadmap/queues/DEPENDENCY_QUEUES.md`
- `docs/reference/DOCUMENTATION_STANDARD.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not spawn agents for simple single-file work.
- Do not let workers edit overlapping files.
- Do not skip validation after writes.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
