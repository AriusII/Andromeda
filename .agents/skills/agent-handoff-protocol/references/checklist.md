# Agent Handoff Protocol Checklist

## Use this checklist for

Prepare handoffs between Codex agents with exact task boundaries, inputs, constraints, expected outputs, and validation. Use when a master agent delegates to specialized subagents.

## Core checklist

| Area | Check |
|---|---|
| Scope | The task has a bounded target and a clear output. |
| Evidence | Relevant project files, docs, or official references are identified. |
| Invariants | Andromeda strict-boundary rules are preserved. |
| Rust safety | Unsafe, panic, native layout, and unbounded async risks are addressed when code is involved. |
| Recovery | Durable or persisted behavior has an explicit recovery story. |
| Observability | Critical decisions produce traces, metrics, diagnostics, or audit records. |
| Validation | Tests, scripts, or review gates are named. |
| Rollback | Destructive or risky work has a rollback or containment path. |

## Focus points

Use these focus points for this skill:

```text
task contract, evidence bundle, output format, validation gate
```

## Rejection criteria

Reject or escalate when:

- The task requires bypassing Procedure contracts, WAL, catalog versioning, IAM, or audit.
- The design depends on hidden runtime state.
- The change cannot be tested or explained.
- The work expands beyond the user's requested scope without a clear reason.
- A C4/C5 behavior is introduced without crash/recovery or security validation.

## Recommended evidence format

```text
Source:
Finding:
Implication:
Decision:
Validation:
```

## Completion criteria

The skill is complete when the output is actionable, bounded, and connected to validation evidence.
