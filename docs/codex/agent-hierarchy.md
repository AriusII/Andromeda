# Agent Hierarchy

## Purpose

Define master-to-specialist delegation for Codex.

## Master agents

- `roadmap-master-orchestrator`
- `codebase-cleanup-commander`
- `mega-refactor-coordinator`
- `release-readiness-coordinator`
- `codex-routing-architect`

## Specialist groups

| Group | Agents |
|---|---|
| Context and prompt | `context-intake-analyst`, `context-pack-builder`, `prompt-refinement-specialist` |
| Codex tooling | `codex-skill-maintainer`, `hook-policy-implementer`, `agent-registry-maintainer`, `prompt-library-curator` |
| Rust cleanup | `rust-cleanup-refactor-architect`, `rust-dead-code-excavator`, `orphan-detection-specialist`, `file-size-split-enforcer` |
| Rust safety/performance | `rust-memory-safety-auditor`, `rust-unsafe-audit-agent`, `rust-performance-optimizer`, `rust-simd-kernel-engineer` |
| Andromeda core | `wal-implementation-engineer`, `storage-segment-format-engineer`, `srpl-parser-binder-engineer`, `optimizer-plan-engineer` |
| Security/operations | `security-implementation-engineer`, `hadr-failover-engineer`, `backup-restore-forensic-engineer`, `observability-forensic-engineer` |

## Handoff contract

Every handoff must include:

```text
Task
Input evidence
Relevant invariants
Allowed scope
Forbidden scope
Expected output
Validation gate
```
