# Prompt Routing Model

## Purpose

Route user prompts to agents and skills.

## Routing steps

1. Identify task type.
2. Identify subsystem.
3. Identify risk.
4. Select central agent.
5. Load skills.
6. Select read-only auditor if the subsystem is critical.
7. Execute or review.

## Examples

| Prompt type | Agent | Skills |
|---|---|---|
| Large roadmap | `roadmap-master-orchestrator` | `roadmap-objective-tracing`, `roadmap-task-decomposition` |
| Cleanup/refactor | `codebase-cleanup-commander` | `rust-clean-code-refactor`, `rust-orphan-detection` |
| WAL design | `wal-implementation-engineer` | `wal-record-design`, `wal-crash-recovery-testing` |
| SRPL language | `srpl-language-specifier` | `srpl-language-design`, `relational-algebra-law-check` |
| Hooks | `hook-policy-implementer` | `hook-design-governance`, `hook-script-hardening` |
