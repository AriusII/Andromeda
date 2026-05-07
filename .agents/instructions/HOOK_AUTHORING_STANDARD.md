# Hook Authoring Standard

## Purpose

Define how lifecycle hooks enforce Andromeda policy.

## Hook classes

- Prompt gates validate user requests before processing.
- Tool gates validate tool parameters before execution.
- Artifact gates validate generated files after write operations.
- Stop gates check whether the task is actually complete.
- Session hooks load project context.

## Rules

- Hooks should be deterministic.
- Hooks must fail closed for destructive operations.
- Hooks must explain why an operation is blocked.
- Hooks must avoid heavy architectural reasoning.
- Hooks must log enough evidence for audit.
