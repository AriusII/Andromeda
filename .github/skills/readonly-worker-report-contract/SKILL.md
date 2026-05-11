---
name: readonly-worker-report-contract
description: Define read only Copilot worker reports when analysis worker scout audit inventory or no edits keywords appear.
license: MIT
---

# readonly-worker-report-contract

## When to use
Use for read-only `/agent` missions that inspect code, docs, tests, CI, dependencies, or architecture and must not modify files. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill readonly-worker-report-contract` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Make analysis workers produce one complete report that the orchestrator can trust, compare, and cite. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Give the worker a bounded question, repository path, forbidden edits, reference docs, search/tool preferences, and exact output path `.work/copilot-cli/<task-slug>/analysis/<worker>.md`.
2. Require sections: Mission, Scope, Files inspected, Commands/searches run, Findings, Evidence, Risks, Recommendations, and Confidence.
3. Tell workers to cite relative paths and line numbers where possible and to distinguish confirmed facts from hypotheses.
4. Do not ask read-only workers to write code or change reports outside their one assigned file.
5. After completion, read the report, extract actionable items, and update the dependency queue.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/reference/DOCUMENTATION_STANDARD.md`
- `docs/roadmap/EXECUTION_METHOD.md`
- `docs/roadmap/queues/DEPENDENCY_QUEUES.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- One worker, one report.
- No edits by read-only workers.
- No broad repository wandering beyond mission scope.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
