# MCP Awareness Without Dependency

## Status

Accepted

## Context

Andromeda uses Codex as an engineering accelerator for a mission-critical Rust database engine and SRPL language. The tooling must be discoverable, precise, safe, and maintainable.

## Decision

Mention rust-lsp and rust-rover-ide as optional tools but do not configure MCP in this package.

## Consequences

The package remains extractable and useful without access to MCP servers.

## Validation

Run:

```bash
python3 .codex/scripts/validate_codex_tooling.py
```

## Revision criteria

Revise this ADR if official Codex configuration, agent, skill, or hook conventions change.
