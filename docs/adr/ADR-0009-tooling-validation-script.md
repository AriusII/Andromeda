# Tooling Validation Script

## Status

Accepted

## Context

Andromeda uses Codex as an engineering accelerator for a mission-critical Rust database engine and SRPL language. The tooling must be discoverable, precise, safe, and maintainable.

## Decision

Ship .codex/scripts/validate_codex_tooling.py with the package.

## Consequences

Provides a local, deterministic validation gate for agents, skills, and hooks.

## Validation

Run:

```bash
python3 .codex/scripts/validate_codex_tooling.py
```

## Revision criteria

Revise this ADR if official Codex configuration, agent, skill, or hook conventions change.
