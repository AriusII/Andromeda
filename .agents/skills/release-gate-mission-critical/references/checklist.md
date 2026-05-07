# Release Gate Mission Critical Checklist

## Use this checklist for

Define release gates for C4/C5 components: WAL, recovery, storage, security, QUIC/RPC, catalog, and Procedure contracts. Use before release or major merge.

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
release criteria, blockers, evidence
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
