# Andromeda AI Operating Pack 2026

**Version:** 0.1.0  
**Date:** 2026-05-03  
**Language:** American English  
**Style:** Microsoft documentation style  
**Project:** Andromeda — Modern Transactional Relational Database System, SRPL, Rust, QUIC, Protobuf, storage hot/cold,
deterministic and mission-critical design.

## Purpose

This archive provides a project-specific AI operating layer for Andromeda. It includes reusable instructions, agent
definitions, focused skills, hooks, prompt templates, quality gates, schemas, and workflows.

The pack is intentionally strict. It exists to keep AI-assisted work aligned with the Andromeda doctrine:

- Procedure-only execution.
- No ad hoc SQL application surface.
- QUIC transport.
- Protobuf contracts without gRPC.
- Strong typing, deterministic behavior, versioned contracts, and observable decisions.
- Rust-first implementation with controlled `unsafe`.
- WAL-before-visible-commit transaction semantics.
- Recovery, audit, and security before optimization.

## How to use this archive

1. Start with `docs/01_AI_OPERATING_ARCHITECTURE.md`.
2. Review `instructions/` and select the project instruction set that matches the workstream.
3. Use `.claude/agents/` as specialized subagent definitions.
4. Use `.claude/skills/` as focused model-invoked capabilities.
5. Use `.claude/settings.json` and `hooks/scripts/` as a guarded Claude Code hook baseline.
6. Use `prompts/` for task briefs and repeatable review requests.
7. Use `registries/` to understand which agents consume which skills and which hooks guard which workflows.

## Local V0 Rust commands

The Rust workspace includes a local V0 recoverable vertical prototype. It is not a production database runtime, network
server, or full storage engine.

```powershell
cargo run -p andromeda-cli -- vertical-v0 --wal "$env:TEMP\andromeda-v0-vertical.wal"
cargo run -p andromeda-cli -- recovery-inspect "$env:TEMP\andromeda-v0-vertical.wal"
cargo run -p andromeda-cli -- protocol-smoke --detail
```

`vertical-v0` executes the current `Inventory.ReserveStock` path through the local V0 SRPL/FileWal flow and writes a
mono-segment WAL file. `recovery-inspect` prints the durable prefix, replay LSNs, ignored transactions, and forensic
boundary status for that WAL. `protocol-smoke --detail` checks local payload/frame lockstep and result-stream ordering
without opening network sockets.

## Design rule

Do not copy a full agent prompt into a skill. Agents orchestrate work. Skills provide narrow repeatable procedures.
Hooks enforce policy boundaries.

## Contents

| Area                   | Path                                      | Purpose                                                                     |
|------------------------|-------------------------------------------|-----------------------------------------------------------------------------|
| Research basis         | `docs/00_RESEARCH_BASIS.md`               | Source-grounded basis for prompts, agents, hooks, and skills.               |
| Operating architecture | `docs/01_AI_OPERATING_ARCHITECTURE.md`    | Explains how instructions, agents, skills, and hooks fit together.          |
| Instructions           | `instructions/`                           | Stable cross-agent policies and standards.                                  |
| Agents                 | `.claude/agents/`                         | Specialized AI workers for Andromeda workstreams.                           |
| Skills                 | `.claude/skills/`                         | Focused capabilities with discoverable `SKILL.md` files.                    |
| Hooks                  | `.claude/settings.json`, `hooks/scripts/` | Lifecycle guardrails for prompts, tool calls, outputs, and stop conditions. |
| Prompts                | `prompts/`                                | Reusable prompt templates.                                                  |
| Workflows              | `workflows/`                              | Repeatable end-to-end operating procedures.                                 |
| Registries             | `registries/`                             | Agent-skill-hook mapping.                                                   |
| Schemas                | `schemas/`                                | JSON schemas for validating future pack assets.                             |

## Important limitations

This pack is not a substitute for formal engineering review, Rust compilation, fuzz testing, crash-recovery testing,
cryptographic review, or database correctness proofs. It is an AI operating layer intended to increase consistency,
reduce drift, and keep work aligned with the project doctrine.
