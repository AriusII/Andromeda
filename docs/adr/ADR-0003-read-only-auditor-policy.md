# Read-Only Auditor Policy

## Status

Accepted

## Context

Andromeda uses Codex as an engineering accelerator for a mission-critical Rust database engine and SRPL language. The tooling must be discoverable, precise, safe, and maintainable.

## Decision

Auditors, reviewers, critics, guardians, threat-modelers, and source-grounding agents use read-only sandbox mode.

## Consequences

Reduces risk that review agents mutate the repository while evaluating critical changes.

## Validation

Run:

```bash
python3 .codex/scripts/validate_codex_tooling.py
```

## Revision criteria

Revise this ADR if official Codex configuration, agent, skill, or hook conventions change.
