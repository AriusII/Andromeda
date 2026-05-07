# Research Cross-Check

## Purpose

Record the source basis for the Codex tooling package.

## Codex source basis

The package aligns with observable Codex conventions:

- Root `AGENTS.md` guidance.
- Project `.codex/config.toml`.
- Project `.codex/agents`.
- Project `.agents/skills`.
- `SKILL.md` with `name` and `description`.
- Hook events and command hooks.

## Andromeda source basis

The package aligns with Andromeda doctrine:

- RPC-only application surface.
- Typed Procedure contracts.
- SRPL strict language direction.
- WAL/MVCC/recovery as C5.
- QUIC/RPC/security surfaces.
- Optimizer/statistics/evidence as bounded adaptive internals.
- Rust 2024 systems-engineering posture.
- Cleanup and refactor quality charter.

## Freshness note

This package was produced from repository-accessible Codex source files and available Andromeda project material in the current session. If Codex hook or agent schemas change, rerun validation and update `.codex/config.toml`, `.codex/hooks.json`, and `.codex/scripts/validate_codex_tooling.py`.
