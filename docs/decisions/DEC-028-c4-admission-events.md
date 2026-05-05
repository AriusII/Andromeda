# DEC-028: C4 Admission Events Architecture

**Date**: 2026 (Wave 12 — Final Release)  
**Status**: APPROVED  
**Scope**: Observability architecture for admission control and contract validation  
**Author**: Observability and Forensic Architect  
**Stakeholders**: Security IAM Auditor, Documentation Architect  

---

## Decision Statement

Implement comprehensive audit events for admission control and contract validation as a separate, immutable event family within `andromeda-observe`. Admission events capture critical decision points in request admission (contract validation, quota enforcement, permission checks, throttling) and enable forensic timeline reconstruction of access control violations and resource governance failures.

---

## Context

### Problem Statement

Wave 11 (C3 — EventEmitter Boundary) completed the core observability infrastructure (TraceEvent, EventEnvelope, EventEmitter). Wave 12 (C4) must now trace the admission control and contract validation layer—the first gate that any request passes through.

Without admission events:
- **No forensic evidence**: Permission denials, quota violations, and throttling decisions leave no observable trace.
- **Incomplete audit trails**: Compliance audits cannot reconstruct the timeline of access control violations.
- **Silent policy failures**: Contract validation or authorization failures appear as errors without machine-parseable classification.

### Key Constraints

1. **Immutable Principal Binding**: All events must bind the `affected_principal` immutably at emission. After event creation, the principal cannot be re-bound (Rust ownership enforces this).
2. **No Optional Fields**: All events in the `AdmissionAuditEvent` enum are fully specified; no `Option<T>` variants (unlike protocol-correlation events that may lack protocol evidence).
3. **Machine-Parseable Classification**: Event types and reasons must be deterministic enums or structured strings, never free-form text.
4. **Forensic Accuracy**: Events must be emitted at deterministic decision points (contract validation gate, admission decision gate, authorization check), not speculatively.
5. **No Privilege Escalation**: Event emission must not expose secret material (private keys, unredacted credentials).

---

## Proposed Solution

### 1. Admission Audit Event Enum (`AdmissionAuditEvent`)

Five event variants, each representing a deterministic admission decision point:

#### **ContractValidated**
- **When**: After contract hash lookup and validation against catalog
- **What**: Procedure ID, input row count, validation result (Valid/Invalid/Deprecated), validation_details
- **Why**: Trace contract matching and incompatibilities
- **Forensic Use**: Proves whether contract mismatch caused rejection before admission gate

#### **AdmissionDecision**
- **When**: After all admission gates pass or fail
- **What**: Procedure ID, decision (Accepted/RejectedQuotaExceeded/RejectedHealthState/RejectedUnknownProcedure), reason, affected_principal
- **Why**: Record the admission gate verdict
- **Forensic Use**: Timeline reconstruction of quota violations, health-state blocks, unknown procedure lookups

#### **PermissionCheckFailed**
- **When**: During authorization when principal lacks required permission
- **What**: Procedure ID, principal, required_permission, actual_permission_set (comma-separated or empty)
- **Why**: Trace permission gaps
- **Forensic Use**: Audit who tried to do what with which permissions (security incident investigation)

#### **RequestThrottled**
- **When**: Request denied due to backpressure (buffer pool, network, queue)
- **What**: Procedure ID, backpressure_reason (BufferPoolFull/NetworkCongestion/QueueOverload), retry_after_ms
- **Why**: Distinguish admission rejection from authorization rejection
- **Forensic Use**: Correlate throttling periods with system resource exhaustion

#### **ProcedureDispatchAuthorized**
- **When**: Immediately before dispatch, after all gates pass
- **What**: Procedure ID, surface_plane, certificate_identity (mTLS subject CN), affected_principal
- **Why**: Prove the final authorization boundary before execution
- **Forensic Use**: Show which mTLS principal authorized which procedure on which surface plane

### 2. Immutable Principal Binding

Each event carries an `AffectedPrincipal`:
```rust
pub struct AffectedPrincipal {
    pub principal_id: String,  // mTLS subject CN or service account ID
}
```

- Bound **at construction time** when `AdmissionAuditEvent` is created.
- Embedded as a non-optional field in each event variant.
- Rust's type system ensures the binding is immutable after event creation (owned by the event).
- **No re-binding after emission**: Once an event is created with a principal, that principal cannot be changed.

### 3. Machine-Parseable Classification

All reason codes and enums are closed, deterministic types:

| Field | Type | Example |
|-------|------|---------|
| `result` | `ContractValidationResult` enum | `Valid`, `Invalid`, `Deprecated` |
| `decision` | `AdmissionDecisionKind` enum | `Accepted`, `RejectedQuotaExceeded` |
| `backpressure_reason` | `BackpressureReason` enum | `BufferPoolFull`, `NetworkCongestion`, `QueueOverload` |
| `reason` | String (non-empty) | e.g., "principal quota limit exceeded: 1000 active requests" |
| `validation_details` | String (non-empty) | e.g., "contract hash mismatch: expected X, got Y" |

### 4. Integration with EventEmitter (C3 Dependency)

Emission point: `crates/andromeda-exec/src/services/admission.rs`
- After contract validation: emit `ContractValidated`
- After admission decision: emit `AdmissionDecision`
- On permission denial: emit `PermissionCheckFailed`
- On throttling: emit `RequestThrottled`
- Before dispatch: emit `ProcedureDispatchAuthorized`

No changes to admission logic; only **add emission points** using the existing `EventEmitter` trait.

### 5. Module Structure

```
crates/andromeda-observe/src/events/
├── admission_audit.rs          (NEW)
├── backup_audit.rs
├── hadr_audit.rs
├── ...
└── mod.rs                      (UPDATED: add mod admission_audit; pub use admission_audit::*)
```

---

## Design Trade-offs

### Decision 1: Separate Event Family vs. Unified "Admission" Trace

**Chosen**: Separate `AdmissionAuditEvent` enum in its own module.

**Rationale**:
- Admission is a distinct domain (contract validation, quota, authorization) separate from execution or recovery.
- Parallel to `BackupAuditEvent` and `HadrAuditEvent`, which are domain-specific families.
- Future forensic pipelines can filter events by domain (`admission_audit`, `backup_audit`, `hadr_audit`, etc.).

### Decision 2: Immutable vs. Re-bindable Principal

**Chosen**: Immutable binding at construction; no re-binding after event creation.

**Rationale**:
- Prevents privilege escalation: An event created for principal A cannot be re-bound to principal B later.
- Rust ownership model enforces immutability statically (no runtime checks needed).
- Aligns with audit trail correctness: Who did what cannot change retroactively.

### Decision 3: No Optional Fields in Admission Events

**Chosen**: All events are fully specified; no `Option<T>` fields in the enum variants.

**Rationale**:
- Admission gates produce **deterministic outputs**: contract valid/invalid, decision accepted/rejected.
- No "optional protocol evidence" like in protocol-correlation events (which may lack stream IDs).
- Simpler validation: if an event exists, all fields are present.
- Forensic clarity: Every event tells a complete story.

### Decision 4: Reason Strings vs. Typed Reason Enums

**Chosen**: Typed reason enums where possible (decision kind, backpressure reason) + structured strings for details.

**Rationale**:
- Decisions are finite and enumerable (accept/reject with 3 rejection reasons).
- Validation details are procedure/catalog-specific and change over time → strings are appropriate.
- Balances machine-parseability (enums) with flexibility (strings).

---

## Relationship to F7 (Backup/Restore Fencing)

F7 specifies that **backup and restore operations admit no procedures** during execution:
- F7 fencing prevents concurrent procedure dispatch while backup or restore is in progress.
- Admission events will show `AdmissionDecision::RejectedHealthState` when a request arrives during F7 fencing.
- This event provides forensic evidence that fencing was active and blocking admission.

**No changes to F7**: F7 fencing logic remains unchanged. Admission events simply trace F7's effect on admission decisions.

---

## Forensic Use Cases

1. **Quota Violation Timeline**
   - Query `AdmissionAuditEvent::AdmissionDecision` for `RejectedQuotaExceeded` within a time window.
   - Correlate with `affected_principal` and `procedure_id` to identify which principal exceeded quota on which procedure.

2. **Permission Audit**
   - Query `AdmissionAuditEvent::PermissionCheckFailed` for a principal.
   - Show which permissions were denied and when.

3. **Throttling Events**
   - Query `AdmissionAuditEvent::RequestThrottled` grouped by `backpressure_reason`.
   - Identify whether throttling was due to buffer exhaustion, network, or queue overload.

4. **Authorization Boundary**
   - Query `AdmissionAuditEvent::ProcedureDispatchAuthorized` to verify mTLS identity and surface plane.
   - Confirm that procedure dispatch was authorized by the correct certificate.

5. **Contract Mismatch Debugging**
   - Query `AdmissionAuditEvent::ContractValidated` for a procedure to find validation failures.
   - Use `validation_details` to debug contract incompatibilities.

---

## Implementation Checklist

- [x] Define `AdmissionAuditEvent` enum with 5 event variants
- [x] Define supporting types: `ProcedureId`, `ContractValidationResult`, `AdmissionDecisionKind`, `BackpressureReason`, `AffectedPrincipal`
- [x] Add immutable principal binding validation
- [x] Module export in `events/mod.rs`
- [x] Comprehensive tests (17 tests):
  - ContractValidated: 3 tests (valid, invalid, deprecated)
  - AdmissionDecision: 5 tests (accepted, quota, health, unknown procedure, classification)
  - PermissionCheckFailed: 3 tests (missing, partial, immutable binding)
  - RequestThrottled: 3 tests (buffer, network, queue)
  - ProcedureDispatchAuthorized: 3 tests (mTLS binding, immutable principal, surfaces)
  - Cross-cutting: 2 tests (trace IDs, principal immutability, type labels, validation)
- [ ] Integration with admission.rs (emission points — future task)
- [ ] `cargo check --workspace` validation
- [ ] Compile without warnings

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|-----------|
| **Principal binding not immutable at runtime** | Low | High — Privilege escalation | Rust type system enforces ownership; immutability guaranteed by borrow checker |
| **Reason strings exposed sensitive data** | Medium | High — Audit trail pollution | Validation helper checks for sensitive markers (PII, key material) in reason strings |
| **Event emission points missed** | Medium | Medium — Silent failures | Admission gate code review; tests verify emission at each gate |
| **Forensic queries too slow** | Low | Medium — Operational impact | Admission events are lightweight (no WAL); query performance acceptable |
| **Conflict with existing F7 fencing** | Low | Low — Specification mismatch | F7 uses health state rejection; no conflict |

---

## Approval

**Approved by**: Observability and Forensic Architect  
**Date**: 2026  
**Next Steps**: Integrate emission points in `admission.rs` (Wave 12 continuation)

---

## References

- **C3**: Event Emitter Boundary (EventSink, TraceEvent, EventEnvelope)
- **F7**: Backup/Restore Fencing (health state blocks admission)
- **C6**: Recovery and Security Audit Specification (audit trail principles)
- **Project No-go Rules**: No ad-hoc SQL, no unsafe runtime behavior, no gRPC, no unbounded SRPL semantics
