---
name: agent-skill-routing
description: Choose direct work skill loading or agent dispatch when routing Copilot CLI tasks by scope risk and available tools keywords appear.
license: MIT
---

# agent-skill-routing

## When to use
Use when deciding whether to handle work directly, invoke `/skill <name>`, dispatch `/agent <name>`, use the code-review agent, or avoid agents for a simple lookup. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill agent-skill-routing` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Route each task to the cheapest reliable execution mode while preserving context, safety, and validation. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. First check if a specialized skill matches the trigger; load it with `/skill <name>` and follow its process.
2. Do direct work for single-file reads, simple edits, or known commands where extra agents add overhead.
3. Use read-only agents for broad independent analysis, code-review agents for high-signal review, and write agents only for bounded non-overlapping missions.
4. Include complete context, repository path, constraints, reference docs, expected output, and validation commands in every agent prompt.
5. After agents finish, inspect results critically; do not copy recommendations blindly.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/roadmap/EXECUTION_METHOD.md`
- `docs/reference/DOCUMENTATION_STANDARD.md`
- `docs/testing/CI_GATES.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not dispatch agents speculatively.
- Do not ask multiple agents to edit the same files.
- Do not ignore available code intelligence tools for Rust symbol questions.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
