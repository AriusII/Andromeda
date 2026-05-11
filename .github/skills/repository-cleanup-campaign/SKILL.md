---
name: repository-cleanup-campaign
description: Plan and execute broad workspace cleanup campaigns when repository cleanup hygiene campaign or many warnings keywords appear.
license: MIT
---

# repository-cleanup-campaign

## When to use
Use for multi-crate cleanup, warning reduction, stale artifact removal, broad quality campaigns, or requests that need several smaller cleanup skills coordinated. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill repository-cleanup-campaign` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Convert broad cleanup intent into a safe campaign with read-only discovery, bounded edit batches, validation, and final consolidation. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Start with `/skill task-scope-bounding` and create a task slug for `.work/copilot-cli/<task-slug>/` reports if artifacts are needed.
2. Dispatch read-only `/agent` workers by independent area: dead code, boundaries, duplicates, naming, tests, and manifests.
3. Build a dependency queue so write workers never collide on the same files.
4. Apply changes in small batches and run the narrowest meaningful validation after each batch, then full canonical gates when feasible.
5. Produce a final summary with changed files, removed artifacts, retained risks, and commands run.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/architecture/REPOSITORY_ARCHITECTURE.md`
- `docs/testing/CI_GATES.md`
- `docs/reference/DOCUMENTATION_STANDARD.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not wander outside the requested campaign.
- Do not combine cleanup with feature delivery.
- Do not let parallel workers edit overlapping files.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
