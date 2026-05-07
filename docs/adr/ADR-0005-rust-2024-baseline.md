# Rust 2024 Baseline

## Status

Accepted

## Context

Andromeda uses Codex as an engineering accelerator for a mission-critical Rust database engine and SRPL language. The tooling must be discoverable, precise, safe, and maintainable.

## Decision

Use Rust 2024 Edition and modern stable Rust policy for Andromeda guidance.

## Consequences

Aligns Codex with current systems Rust practices and explicit unsafe boundaries.

## Validation

Run:

```bash
python3 .codex/scripts/validate_codex_tooling.py
```

## Revision criteria

Revise this ADR if official Codex configuration, agent, skill, or hook conventions change.
