# Project Context Intake Checklist

## Use this checklist for

Extract the minimum sufficient Andromeda context from user prompts, repository files, uploaded documents, and local evidence. Use before major architecture, refactor, or implementation tasks.

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
scope, source set, invariants, missing evidence, assumptions
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
