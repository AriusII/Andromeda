---
name: implementation-direct-change-protocol
description: Guide bounded direct Copilot implementation changes when implement fix change patch or small edit keywords appear.
license: MIT
---

# implementation-direct-change-protocol

## When to use
Use for direct implementation requests that are small enough for the main Copilot CLI agent without worker orchestration. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill implementation-direct-change-protocol` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Deliver surgical code changes in validated slices while avoiding broad rewrites and unrelated cleanup. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Identify the smallest owner and expected behavior; inspect surrounding code and tests before editing.
2. Prefer ecosystem and refactoring tools over manual broad changes; for Rust, use code intelligence for definitions/references.
3. Edit one coherent slice, format it, and run the narrowest meaningful cargo command first.
4. Escalate to `/skill copilot-orchestration-protocol` if the scope grows beyond a bounded slice.
5. Summarize changed files, validation, and any residual risk in the final response.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/project/FEATURE_ACCEPTANCE_GATE.md`
- `docs/testing/TEST_STRATEGY.md`
- `docs/testing/CI_GATES.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not modify unrelated code.
- Do not skip tests after code edits.
- Do not change docs or contracts unless directly required by the implementation.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
