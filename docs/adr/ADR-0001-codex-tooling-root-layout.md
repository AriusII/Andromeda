# Codex Tooling Root Layout

## Status

Accepted

## Context

Andromeda uses Codex as an engineering accelerator for a mission-critical Rust database engine and SRPL language. The tooling must be discoverable, precise, safe, and maintainable.

## Decision

Place AGENTS.md, .codex, .agents, and docs/codex directly at the repository root.

## Consequences

This lets Codex discover root instructions, agents, skills, hooks, and prompts without an intermediate folder.

## Validation

Run:

```bash
python3 .codex/scripts/validate_codex_tooling.py
```

## Revision criteria

Revise this ADR if official Codex configuration, agent, skill, or hook conventions change.
