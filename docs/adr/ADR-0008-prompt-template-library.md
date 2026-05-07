# Prompt Template Library

## Status

Accepted

## Context

Andromeda uses Codex as an engineering accelerator for a mission-critical Rust database engine and SRPL language. The tooling must be discoverable, precise, safe, and maintainable.

## Decision

Maintain reusable prompts under .codex/prompts.

## Consequences

Gives agents repeatable task framing for roadmap, cleanup, WAL, SRPL, security, and release workflows.

## Validation

Run:

```bash
python3 .codex/scripts/validate_codex_tooling.py
```

## Revision criteria

Revise this ADR if official Codex configuration, agent, skill, or hook conventions change.
