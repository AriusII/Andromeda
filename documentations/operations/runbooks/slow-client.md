# Slow Client Runbook

## Purpose

Use this runbook when a client, session, or result stream cannot consume responses fast enough and begins to create protocol, memory, WAL, audit, or scheduler pressure.

## Scope

This runbook covers containment for slow clients on application or administrative sessions. It emphasizes bounded backpressure, session evidence, and protection of WAL, recovery, audit, and security-critical work.

## Non-goals

This runbook does not define a new ResultStream protocol, authorize unbounded spooling, or permit application traffic to call Administration or HA/DR controls. It also does not use slow-client handling as a reason to drop required audit or recovery evidence.

## Prerequisites

- The operator can identify RequestId, SessionId, principal, surface, Procedure contract identity, result stream sequence state, and resource quota state.
- The operator can distinguish Application Surface traffic from Administration Surface and HA/DR Cluster Surface traffic.
- Quota and backpressure policy thresholds are available or the incident is treated as a planned policy gap.

## Status

| Area | Status | Evidence or limitation |
| --- | --- | --- |
| Surface separation | Implemented and documented as a security boundary across core principal scope and specs. | `documentations/specs/SecurityAdmission_v0.md`; `crates/andromeda-core/src/principal/surface_scope.rs`. |
| Audit event family requirements | Contract preview with durable audit implementation evidence in observe and exec areas. | `documentations/specs/AuditLedger_v0.md`; audit tests under `crates/andromeda-cli` and `crates/andromeda-exec`. |
| ResultStream backpressure procedure | Contract preview. | This runbook describes expected operations, not a proven end-to-end transport control. |
| Operator throttling commands | Contract preview unless proven by CLI tests. | Use dry-run/verify-only wording for rehearsals. |

## Findings Ordered By Severity

| Severity | Finding | Exact invariant or contract affected | Concrete remediation |
| --- | --- | --- | --- |
| Critical | Slow-client response handling must not block WAL, rollback, recovery, MVCC short-visibility, catalog publication, or security-critical work. | Invariants 3, 5, and 10; WAL/recovery and security quality gates. | Apply bounded backpressure at the session/stream boundary and protect critical runtime queues. |
| High | Unbounded spooling can turn a slow client into memory or temp-storage false truth. | Invariant 4: Do not treat RAM or temp storage as truth. | Spool only within explicit quotas and never use spooled bytes as commit, recovery, or audit truth. |
| High | Closing a slow session without evidence can hide contract, security, or protocol failures. | AuditLedger v0 admission, security, and generic audit families; Procedure contract boundary. | Preserve RequestId, SessionId, principal, surface, contract hash, sequence boundary, close reason, and quota state. |
| Medium | Slow administrative clients can delay incident work if mixed with application traffic. | Invariant 8: Do not expose Administration or HA/DR capabilities through Application Surface. | Keep administrative incident channels separate and prioritize recovery/audit traffic by policy. |

## Procedure

1. Identify the slow client.
   - Capture RequestId, SessionId, principal, surface, Procedure contract identity, stream id, current sequence, pending bytes, and quota state.
   - Mark collection as dry-run/verify-only.

2. Protect critical paths.
   - Verify WAL flush, rollback, recovery, catalog publication, and security admission queues are not waiting on the slow client.
   - If a dependency exists, classify it as a release-blocking design defect.

3. Apply bounded backpressure.
   - Reduce batch size for the affected stream.
   - Pause delivery for low-priority responses when policy allows.
   - Use controlled spooling only when quota and retention policy are explicit.
   - Do not spill secret-bearing payloads without the security owner-approved policy.

4. Decide whether to close the session.
   - Close the session after policy thresholds for lag, quota, or abusive behavior are exceeded.
   - Emit audit evidence before or as part of closure.
   - Do not re-execute a Procedure to rebuild a response unless the typed Procedure contract and transaction semantics explicitly allow a safe retry.

5. Recover normal flow.
   - Confirm stream sequence accounting remains consistent.
   - Confirm no required audit, recovery, or Procedure completion evidence was dropped.
   - Reopen admission only when the affected queues are below policy thresholds.

## Validation

Required before merge for runtime changes related to this runbook:

- Result stream sequence tests for partial sends, client disconnect, retry, completion, and error conversion.
- Backpressure tests showing bounded memory and bounded temp storage.
- Security tests proving Application Surface sessions cannot invoke administrative or HA/DR actions.
- Audit tests proving session throttling, closure, and policy rejection evidence are emitted.
- WAL/recovery regression tests proving slow clients do not delay durable commit or recovery evidence.

Suggested commands when related code changes exist:

```powershell
cargo test -p andromeda-quic
cargo test -p andromeda-rpc-protocol
cargo test -p andromeda-exec exec_audit_completion_validation
cargo test -p andromeda-core principal_contract_projection
```

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Memory grows with pending client responses. | Spooling or buffering lacks a quota. | Enforce quota, reduce batch size, or close the session with audit evidence. |
| WAL latency rises during slow-client incident. | Critical path is coupled to response delivery. | Treat as a C5 defect and isolate WAL/commit from stream delivery. |
| Operator cannot identify the surface. | Admission evidence is incomplete. | Fail closed for privileged operations and require security/admission trace review. |

## References

- `AGENTS.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `documentations/specs/AuditLedger_v0.md`
- `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md`
- `.agents/instructions/QUALITY_GATES.md`
