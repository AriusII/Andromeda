---
name: copilot-agent-authoring
description: Author Copilot CLI agent files when .agent.md agent frontmatter tool aliases worker or sub-agent keywords appear.
license: MIT
---

# copilot-agent-authoring

## When to use
Use when creating or reviewing Copilot CLI `.agent.md` files, sub-worker definitions, tool aliases, delegation contracts, or restrictions for analysis/write workers. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill copilot-agent-authoring` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Create focused agent definitions that are safe to dispatch with `/agent <name>` and compatible with Andromeda's orchestration model. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Define the agent purpose, trigger, allowed scope, read/write capability, expected output, and validation responsibility before writing.
2. Use YAML frontmatter consistently with the Copilot CLI agent format in the repository; keep tool aliases explicit and minimal.
3. Separate read-only analysis agents from write agents; include output contracts under `.work/copilot-cli/<task-slug>/`.
4. Reference relevant skills by name so missions can say `/skill <name>` instead of duplicating every rule.
5. Review agent instructions for over-broad authority, overlapping file ownership, missing validation, or conflicts with Andromeda guardrails.

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
- Do not grant broad write authority by default.
- Do not make agents responsible for secret handling or external services beyond the task.
- Do not let agent definitions bypass Procedure-only, WAL, security, or supply-chain rules.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
