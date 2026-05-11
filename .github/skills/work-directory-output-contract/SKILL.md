---
name: work-directory-output-contract
description: Route Copilot CLI intermediate artifacts under .work/copilot-cli task directories when workdir artifact mission report keywords appear.
license: MIT
---

# work-directory-output-contract

## When to use
Use when a task needs intermediate reports, plans, worker missions, final summaries, durable artifacts, or mentions `.work`, task slug, analysis, plans, missions, or final. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill work-directory-output-contract` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Keep orchestration artifacts organized without polluting source, docs, or crate directories. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Create a deterministic task slug from the request, then use `.work/copilot-cli/<task-slug>/{analysis,plans,missions,final}/`.
2. Put read-only worker reports under `analysis/`, orchestration plans under `plans/`, write-worker mission reports under `missions/`, and consolidated outcome under `final/`.
3. Reference artifact paths in agent prompts and final responses.
4. Keep source edits separate from reports; never use reports as a substitute for code validation.
5. Clean up abandoned scratch files only when they are not needed as evidence.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/reference/DOCUMENTATION_STANDARD.md`
- `docs/roadmap/EXECUTION_METHOD.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not write task artifacts into `docs/`.
- Do not use system temp directories.
- Do not create hidden ad hoc folders when `.work/copilot-cli` is the contract.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
