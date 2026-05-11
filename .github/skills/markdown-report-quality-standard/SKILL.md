---
name: markdown-report-quality-standard
description: Set Markdown report quality for Copilot CLI work artifacts when report analysis final summary or .work keywords appear.
license: MIT
---

# markdown-report-quality-standard

## When to use
Use for `.work/copilot-cli/<task-slug>/` reports, worker outputs, analysis summaries, final consolidation, audit evidence, and markdown quality reviews. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill markdown-report-quality-standard` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Ensure reports are useful operational artifacts: source-grounded, concise, structured, and action-oriented. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Use clear headings: Scope, Inputs, Methods, Findings, Evidence, Recommendations, Validation, Risks, and Next Steps as applicable.
2. Cite files, commands, docs, and worker reports with relative paths; distinguish observed facts from assumptions.
3. Prefer tables for inventories and decision matrices; keep prose concise and avoid raw log dumps unless essential.
4. For agent workers, require exactly one report in the assigned directory and no repository edits for read-only missions.
5. Final reports should reconcile conflicts, name blockers, and list commands that actually ran.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/reference/DOCUMENTATION_STANDARD.md`
- `docs/STYLE_GUIDE.md`
- `docs/reference/MARKDOWN_VALIDATION_REPORT.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not create planning markdown unless the task calls for artifacts.
- Do not invent evidence or cite unread files.
- Do not place reports outside `.work/copilot-cli/<task-slug>/` when this contract applies.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
