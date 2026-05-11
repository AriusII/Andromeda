---
name: copilot-skill-authoring
description: Author Copilot CLI skills when skill authoring SKILL.md frontmatter tool-gating or trigger keywords appear.
license: MIT
---

# copilot-skill-authoring

## When to use
Use when creating or reviewing `.github/skills/<skill-name>/SKILL.md`, translating Codex skills to Copilot CLI, or checking frontmatter, descriptions, triggers, and doc-style skill rules. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill copilot-skill-authoring` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Produce precise Copilot CLI skills that load well with `/skill <name>` and guide agents without over-constraining tools in frontmatter. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Use lowercase hyphenated directory names and make the frontmatter name exactly match the directory.
2. Frontmatter must include `name`, a one-sentence trigger-rich `description`, and `license: MIT`; do not add `allowed-tools` for doc-style skills unless the user explicitly requires tool-gated skills.
3. Write body sections `When to use`, `Purpose`, `Process`, `Expected output`, `Reference docs`, `Guardrails`, and `Andromeda baseline`.
4. Mention Copilot CLI workflow: `/skill <name>`, `/agent <name>` for workers, and available tools such as code intelligence/rust-lsp, grep/glob/view, powershell, sql, GitHub, and task agents.
5. Verify referenced docs exist and run a directory listing of `.github/skills` after creation.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/README.md`
- `docs/reference/DOCUMENTATION_STANDARD.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`
- `docs/architecture/ENGINE_OVERVIEW.md`

## Guardrails
- Do not copy vague agent instructions into a skill.
- Do not add `allowed-tools` to doc-style skills.
- Do not reference nonexistent docs or write skills outside `.github/skills/<name>/SKILL.md`.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
