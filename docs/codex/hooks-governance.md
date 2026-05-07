# Hooks Governance

## Purpose

Define hook policy for Andromeda Codex usage.

## Supported hook events

- `SessionStart`
- `UserPromptSubmit`
- `PreToolUse`
- `PermissionRequest`
- `PostToolUse`
- `Stop`

## Rules

- Every hook command must have a timeout.
- Every hook command must have a status message.
- Hooks must not require secrets.
- Hooks must not depend on network access.
- Hooks must fail safely.
- Hooks should add context or block clearly unsafe actions. They should not become hidden business logic.

## PreToolUse policy

Block:

- broad destructive commands,
- commands that expose secrets,
- global `target-cpu=native` policy,
- broad `cargo clean` without explicit targeting.

## Stop policy

The Stop hook should remind the agent to provide artifact links, validation, citations, and residual risk. It should avoid excessive blocking unless a clear output contract is violated.
