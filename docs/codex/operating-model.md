# Codex Operating Model for Andromeda

## Purpose

Define how Codex should operate inside Andromeda as a mission-critical Rust database-engine project.

## Operating model

Codex should process work in this order:

1. Understand the request.
2. Classify risk.
3. Load the root AGENTS.md and relevant child AGENTS.md files.
4. Select a central agent.
5. Load precise skills.
6. Execute the smallest safe slice.
7. Validate the result.
8. Report residual risk.

## Agent hierarchy

A central agent may delegate to specialized agents, but the central agent owns integration.

Recommended central agents:

| Request type | Central agent |
|---|---|
| Broad roadmap or project evolution | `roadmap-master-orchestrator` |
| Large cleanup/refactor | `codebase-cleanup-commander` |
| Rust architecture split | `rust-architecture-splitter` |
| Codex tooling changes | `codex-routing-architect` or `codex-skill-maintainer` |
| WAL/storage/recovery | `wal-implementation-engineer` plus read-only auditors |
| SRPL language work | `srpl-parser-binder-engineer` or `srpl-language-specifier` |
| Security/IAM | `security-implementation-engineer` plus auditors |
| Release readiness | `release-readiness-coordinator` |

## Risk posture

Read-only agents should review, critique, and produce findings. Implementation agents may edit repository files. When a task involves C4/C5 behavior, require an auditor pass or explicit validation gate.
