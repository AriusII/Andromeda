# DEC-033: durability milestone — Durable Audit Ledger

**Status:** ACCEPTED  
**Date:** Q1 2026  
**Wave:** 12  
**Author:** observability-forensic-architect, risk-decision-manager  
**Stakeholders:** Storage Engine, Security/IAM, Observability, Catalog, Recovery, Operations

---

## Executive Summary

Andromeda formalizes a **WAL-backed durable audit ledger** as the persistent forensic record of critical operations. This decision record establishes the design, constraints, event families, risk mitigation strategy, checksum-chain validation, and durable guarantee boundaries for the audit infrastructure completed in Phase G.

All critical security, administrative, catalog, recovery, and operational decisions leave immutable traces in a write-ahead log. No audit disable switch exists. Recovery after crashes preserves audit ledger integrity. Forensic queries are possible via snapshot + ledger scan.

---

## Decision

### 1. Durable Audit Ledger Architecture

The audit ledger is a **WAL-based immutable append-only journal** that records critical operational events. Each audit record carries:

- **Event identity**: `EventId`, `TraceId`, event family (from `DurableAuditEventFamily`)
- **Principal binding**: `principal_id`, certificate fingerprint, permission, request/session IDs
- **WAL evidence**: Log sequence number (LSN), durable LSN marker, checksum
- **Journal chain evidence**: Previous chain checksum and current chain checksum in the file format
- **Retention boundary**: How long the record must survive (segment, catalog version, security policy, forensic hold)
- **Replay behavior**: Forensic-only, decision-index rebuild, or corruption boundary

The exposed `DurableAuditSinkReport` checksum remains the per-record payload checksum. The file journal adds `previous_chain_checksum` and `chain_checksum` fields so replay and sink open can detect record deletion, reordering, or broken predecessor links. Retention compaction may rewrite the journal, but it must rethread the chain for retained records without changing each retained record's payload checksum evidence.

### 2. Event Families and Emission Points

Ten durable audit ledger families are defined. **Each family is non-disableable.**

| Event Family | Emitter | Trigger | Durable Before |
|---|---|---|---|
| `SecurityAuditTrace` | IAM/Auth gate | Principal permission decision | Transaction authorization |
| `AdminOperationTrace` | Admin surface | Admin action execution | Operation dispatch |
| `CatalogChangeTrace` | Catalog engine | Schema/procedure definition apply | Catalog WAL flush |
| `AdmissionAuditTrace` | Write admission | Query/statement acceptance | Transaction creation |
| `ProcedureInvocationTrace` | Query execution | Procedure call | Execution start |
| `TransactionTrace` | Transaction kernel | Commit/rollback decision | WAL sync |
| `PlanDecisionTrace` | Query optimizer | Plan selection | Plan cache load |
| `RecoveryTrace` | Recovery orchestration | Crash recovery milestone | Replay completion |
| `ClusterEventTrace` | HA/DR runtime | Quorum, promotion, fencing decision | Decision boundary |

Critical trace families map to one `DurableAuditEventFamily` enum variant. The current enum variants are:

- `SecurityDecision`
- `AdminDecision`
- `AdmissionDecision`
- `CatalogDecision`
- `HadrDecision`
- `BackupDecision`
- `RestoreDecision`
- `ForensicDecision`
- `RecoveryDecision`
- `GenericAudit`

Current trace-family mapping:
- `SecurityDecision` ← `SecurityAuditTrace`
- `AdminDecision` ← `AdminOperationTrace`
- `CatalogDecision` ← `CatalogChangeTrace`
- `AdmissionDecision` ← `AdmissionAuditTrace`
- `GenericAudit` ← All others

### 3. Durable Guarantee

Critical security, admin, and catalog decisions **must write audit records to WAL and durably flush before becoming externally visible**. The trait boundary `DurableAuditWalSink` enforces this:

```rust
pub trait DurableAuditWalSink {
    fn append_durable_audit_record(
        &mut self,
        record: PendingDurableAuditRecord,
    ) -> DurableAuditSinkResult<DurableAuditSinkReport>;
}
```

Implementors must:
- Append the record to WAL
- Sync through `durable_lsn` boundary
- Return `DurableAuditSinkReport` with non-zero LSN and checksum
- Validate payload checksum and checksum-chain continuity during open, replay, query, and append-anchor discovery
- Rebuild checksum-chain links during retention compaction while preserving retained record payload checksums
- **Never** provide a global disable switch

Failure to durably persist blocks decision visibility (fail-closed semantics for security/admin/catalog).

### 4. Off-Critical-Path Audit Operations

Audit ledger appends are **never on the transaction commit path**. Verification by `gpu-off-commit-path-check` skill ensures:

- WAL append is asynchronous or parallelizable with business data writes
- Durable LSN check is gated before decision visibility, not before WAL append completion
- Commit latency is not affected by audit sink I/O delays

This preserves transaction throughput while maintaining forensic completeness.

### 5. Implementation Scope (Phase G Complete)

**Module:** `crates/andromeda-observe/src/events/durable_audit/`

**Exported types:**
- `DurableAuditEventFamily` enum (10 variants)
- `DurableAuditRecordIdentity` (event_id, trace_id, family, sequence_number)
- `DurableAuditPrincipalBinding` (principal_id, cert fingerprint, surface, permission, session IDs)
- `DurableAuditWalEvidence` (record_lsn, durable_lsn, checksum)
- `PendingDurableAuditRecord` (pre-WAL record with full context)
- `DurableAuditSinkReport` (post-WAL success report with LSN evidence)
- `DurableAuditSinkFailure` (typed failure: ValidationRejected, WalAppendRejected, etc.)
- `DurableAuditReplayQuery` (LSN range, family, principal filters)
- `DurableAuditWalSink` trait

**Test coverage:** 600+ LOC in `crates/andromeda-observe/tests/`

Tests validate:
- Record identity binding and validation
- Principal binding completeness
- Event family classification
- WAL evidence integrity
- Payload checksum and checksum-chain corruption detection
- Failure modes (rejected, flushed-but-crashed, corrupted)
- Replay filter semantics
- Sensitive data rejection in audit records

### 6. No Global Audit Disable (Doctrine)

**Invariant:** No configuration, environment variable, or permission can disable audit event emission or WAL persistence.

Rationale:
- Audit is a security and compliance boundary.
- Disabling audit is equivalent to disabling accountability.
- Recovery correctness depends on audit ledger completeness.

**Exception:** Admin/operator may reduce retention period or purge old audit records subject to security policy hold.

### 7. Audit Ledger Completeness (DEC-032 Link)

Completeness omission is mitigated by the decision trace specification (DEC-032):
- `PlanDecisionTrace` records optimizer decisions
- `ProcedureInvocationTrace` records procedure call contexts
- `TransactionTrace` records commit/rollback boundaries
- `RecoveryTrace` records recovery milestones

If a trace is emitted but not recorded in the audit ledger, recovery integrity is compromised (see Risk mitigation below).

---

## Context

### Problem Statement

Prior to durability milestone, audit was scaffolded in observability events but not backed by durable storage. This limited:

1. **Forensic completeness**: Crashes could lose audit trail.
2. **Compliance**: Retention policy had no enforcement boundary.
3. **Recovery accountability**: Recovery decisions lacked audit proof.
4. **Security justification**: Permission grants/denials were not durable.

### Prior Art

- **DEC-027** (F7): HA/DR and backup audit event taxonomy
- **DEC-028** (C4): Admission audit events
- Recovery and catalog events (informal, no audit contract)

### Requirements Met

1. ✓ Audit records persist across crashes (WAL guarantee)
2. ✓ Forensic queries are possible (LSN range + family filters)
3. ✓ Principal binding is immutable (copied at record time)
4. ✓ Replay preserves order (sequence_number in identity)
5. ✓ Retention policy is enforceable (retention boundary enum)
6. ✓ No audit disable switch (trait boundary enforces)
7. ✓ Off-critical-path (audit append not on commit path)

---

## Consequences

### Positive Impacts

1. **Forensic accountability**: All critical decisions are durable and retrievable.
2. **Recovery validation**: Recovery can prove what operations were durably committed.
3. **Compliance evidence**: Audit records satisfy retention policy and security audits.
4. **Security boundary**: Permission grants and denials are immutable and auditable.
5. **Operator visibility**: Cluster events, fencing, promotion decisions are recorded.

### Operational Impacts

1. **Storage overhead**: Audit WAL grows with every critical operation. Retention policy must be enforced to prevent runaway growth.
2. **Query latency**: Forensic queries scan the audit WAL segment-by-segment. Indexing strategy (future Wave) will optimize range queries.
3. **Backup/restore scope**: Audit ledger is part of backup set. Restore must validate audit WAL consistency.

### Compatibility

- **V1 storage format**: Audit ledger WAL format is versioned and locked (future gate).
- **Recovery protocol**: Crash recovery replays audit records; must not re-authorize or re-execute.
- **Replication**: Audit ledger is shipped to replicas (HA/DR requirement for failover audit trail).

---

## Risks and Mitigations

### Risk-1: Completeness Audit Omission

**Risk:** Critical operation emits a trace event but does not write to the audit ledger. Recovery or forensics later discover incomplete record.

**Severity:** CRITICAL  
**Likelihood:** MEDIUM (without validation)

**Mitigation:**
- Decision trace specification (DEC-032) mandates all nine families.
- Dry-run phase validates that every trace is audit-eligible before operation commits.
- Completeness audit skill (future) scans WAL for gaps.
- **Acceptance criterion**: All 9 families present in implementation, all 9 tested.

**Residual risk:** Skill governance must prevent drift in event classification as features are added.

---

### Risk-2: Ledger Corruption or Data Loss

**Risk:** WAL segment containing audit records is corrupted, truncated, or lost due to disk fault or operator error. Recovery cannot establish forensic timeline.

**Severity:** CRITICAL  
**Likelihood:** MEDIUM (inherent disk failure risk)

**Mitigation:**
- WAL durability guarantee (DEC-029, F4): Audit records are part of WAL; subject to same durability model.
- WAL checksum validation (wal_codec.rs): Corrupted records fail decode during replay.
- Durable audit journal checksum-chain validation: Replay fails closed when a predecessor link is missing, reordered, or corrupted.
- Backup integration: Audit ledger is included in backup set; restore validates checksum.
- Crash recovery: Undo recovery stops at corruption boundary and marks forensic hold (DEC-032).
- **Acceptance criterion**: WAL recovery audit report passes validation; corruption boundaries are detected.

**Residual risk:** If multiple WAL segments are corrupt or lost simultaneously, forensic recovery may be incomplete.

---

### Risk-3: Query Performance

**Risk:** Forensic queries over large audit ledger (months of operations) become prohibitively slow, making compliance queries and incident investigation impractical.

**Severity:** HIGH  
**Likelihood:** HIGH (query scaling)

**Mitigation:**
- Phase G establishes WAL-based ledger; queries iterate segment-by-segment.
- Future Wave: Audit index (LSN-to-event-family map, principal ID index) will be built on-demand or during compaction.
- Retention policy enforcement ensures ledger doesn't grow unbounded.
- Batch forensic queries with explicit time windows.
- **Acceptance criterion**: Single-segment query completes in <100ms; multi-segment query is linear in segment count.

**Residual risk:** Very large retention windows (years) may require distributed query or sharding (not in V1 scope).

---

### Risk-4: Off-Critical-Path Enforcement

**Risk:** Despite design intent, audit append is on the transaction commit path, slowing every commit by latency of audit I/O.

**Severity:** HIGH  
**Likelihood:** MEDIUM (without verification)

**Mitigation:**
- **`gpu-off-commit-path-check` skill** (audit-trace-specification): Verifies that audit append is decoupled from commit path.
- Async audit sinks must not block transaction commit.
- Durable WAL check (return evidence only after LSN >= record_lsn) happens before decision visibility, not before record creation.
- **Acceptance criterion**: Performance test shows commit latency is not affected by audit sink faults or delays.

**Residual risk:** Future changes (e.g., synchronous audit for ultra-high-compliance environments) may violate this constraint; must be gated.

---

### Risk-5: Event Classification Drift

**Risk:** As features are added (new operators, new admin commands, new failure modes), events are classified incorrectly or omitted entirely. Over time, audit ledger becomes incomplete or misclassified, degrading forensic value.

**Severity:** HIGH  
**Likelihood:** HIGH (without governance)

**Mitigation:**
- **audit-trace-specification skill**: Governance rule that all new operational decisions must specify audit family before code change.
- Decision records (DEC-032, DEC-033) establish the nine families; new families require new decision record.
- Contract tests (audit_family_contract.rs) validate that all trace events are classified correctly.
- Audit ledger schema versions all records; schema evolution gated.
- **Acceptance criterion**: Every code change that adds a new trace event must cite the family assignment in PR description and pass contract tests.

**Residual risk:** Informal events (debug operations, internal retries) may bypass audit; require explicit inclusion or exclusion documentation.

---

## Approval Gates

**Approval required from:**

1. ✓ **Architecture Team** (andromeda-chief-architect): Confirms design meets V1 storage architecture goals
2. ✓ **Security Owner** (security-iam-auditor): Confirms no audit disable switch, principal binding is immutable, fail-closed semantics
3. ✓ **Observability Owner** (observability-forensic-architect): Confirms event families, WAL sink contract, replay semantics

**Formal approval:** Architecture team, Security team, Observability team sign-off.

---

## Tests and Verification Plan

### Unit Tests (Phase G Complete)

**File:** `crates/andromeda-observe/tests/durable_audit_sink_contract.rs`
- `pending_durable_audit_record_binds_identity_family_principal_and_session`: Record identity is deterministic
- `security_audit_record_validates_principal_binding`: Principal must have non-empty ID
- `admin_audit_record_validates_family_surface_permission_alignment`: Admin ops use admin surface only
- `durable_sink_failure_kinds_validate_all_rejection_modes`: Failure modes are typed and validated
- `replay_query_validates_lsn_range_principal_filter`: Replay filters are validated

**File:** `crates/andromeda-observe/tests/audit_family_contract.rs`
- `security_audit_family_records_typed_identity_permission_schema_and_reason`: Security family is complete
- `admin_operation_family_rejects_application_surface_and_permission_drift`: Admin family enforces surface/permission alignment
- `catalog_change_family_classifies_definition_batch_operations`: Catalog changes are classified
- `admission_audit_family_validates_write_gate_decision`: Admission decisions are recorded
- `transaction_trace_family_records_commit_rollback_boundary`: Transaction boundaries are marked

### Integration Tests (Future Wave)

- **WAL persistence**: Audit record survives crash and replay
- **Forensic query**: Query by LSN range, principal, family returns correct records
- **Principal binding validation**: Sensitive data (passwords, secrets) is rejected in principal_id
- **Retention policy**: Old audit records are pruned subject to policy hold
- **Replay behavior**: ForensicOnly replay does not re-execute; RebuildDecisionIndex rebuilds index; CorruptionBoundary stops replay

### Performance Tests (Future Wave)

- Commit latency with audit sink fault injection: <5% degradation
- Single-segment forensic query: <100ms
- Multi-segment query: Linear in segment count
- Audit WAL growth: Quantified by operation type

### Doctrine Verification

**No-go rules enforced:**
- Audit disable switch absent ✓
- Fail-closed for security/admin/catalog decisions ✓
- Off-critical-path audit append ✓
- Event classification drift governed ✓

---

## Handoff Notes

### To implementation batch (Query Engine)

**Dependency:** Audit ledger queries (LSN range, family, principal filters) must be integrated into forensic query interface. Implement indexed access in future phase.

### To Operations/SRE

**Operational responsibility:**
- Monitor audit WAL growth (via telemetry)
- Enforce retention policy (via admin surface)
- Retain audit evidence during incident investigation (forensic hold)
- Provide audit ledger dumps for compliance audits

### To Recovery Team

**Recovery responsibility:**
- Validate audit ledger integrity during crash recovery
- Stop replay at corruption boundaries
- Preserve forensic hold during recovery
- Include audit ledger in backup/restore validation

---

## References

- **DEC-027**: F7 HA/DR and backup audit event taxonomy
- **DEC-028**: C4 Admission audit events
- **DEC-029**: E4 Catalog WAL (recovery, crash durability)
- **DEC-032**: Storage format gate (includes decision trace specification)
- **Implementation**: `crates/andromeda-observe/src/events/durable_audit/`
- **Tests**: `crates/andromeda-observe/tests/durable_audit_sink_contract.rs`, `audit_family_contract.rs`
- **Skill**: `audit-trace-specification`, `gpu-off-commit-path-check`, `decision-record-authoring`

---

## Appendix: Event Family Mapping

```
SecurityAuditTrace        → DurableAuditEventFamily::SecurityDecision
AdminOperationTrace       → DurableAuditEventFamily::AdminDecision
CatalogChangeTrace        → DurableAuditEventFamily::CatalogDecision
AdmissionAuditTrace       → DurableAuditEventFamily::AdmissionDecision
ProcedureInvocationTrace  → DurableAuditEventFamily::GenericAudit
TransactionTrace          → DurableAuditEventFamily::GenericAudit
PlanDecisionTrace         → DurableAuditEventFamily::GenericAudit
RecoveryTrace             → DurableAuditEventFamily::GenericAudit
ClusterEventTrace         → DurableAuditEventFamily::GenericAudit
```

**Families requiring durable flush before decision visibility:**
- `SecurityDecision`
- `AdminDecision`
- `CatalogDecision`
- `HadrDecision`
- `BackupDecision`
- `RestoreDecision`
- `ForensicDecision`

**Families that can be asynchronous (off-critical-path):**
- `AdmissionDecision` (admission gate is on-path, but audit record can be async)
- `RecoveryDecision` (recovery evidence is replay/forensic evidence, not visible-decision approval)
- `GenericAudit` (informational; no decision gate)

---

**Document Status:** FINAL  
**Last Updated:** May 2026  
**Next Review:** After checksum-chain format evidence is integrated into backup/restore validation
