# Andromeda Decision Records — Master Index

**Last Updated:** Wave 17 (2026-01-XX)  
**Status:** ✅ Reference audit complete (DEC-CLEAN-005)  
**Repository:** `docs/decisions/`

---

## Quick Navigation by Layer

### Foundation Layer (D1–D4)

Transport, identity, and protocol infrastructure:

| ID | Title | Status | Key Invariants | Links |
|----|-------|--------|---|---|
| **DEC-011** | Rust Workspace Topology | ✅ Accepted | Virtual Cargo workspace, boundary separation | Implements: architecture foundation |
| **DEC-012** | Phase 0 Contract Baselines | ✅ Accepted | Std-only contracts, no runtime deps | Enables: all domain contracts |
| **DEC-013** | Local Vertical Prototype | ✅ Accepted | In-memory WAL, contract-before-tx | Validates: orchestration order |
| **DEC-014** | Rust Crate Module Structure | ✅ Accepted | Focused modules, root-level re-exports | Guides: code organization policy |
| **DEC-017** | QUIC Runtime Dependency | ✅ Accepted | Quinn + rustls, deferred feature gate | Selects: network stack (D1) |
| **DEC-018** | mTLS Identity Extraction | ✅ Approved (D3) | Secure QUIC identity binding | Requires: D1 runtime feature |

### Transport/Stream Layer (D5–D7)

QUIC frame/stream handling and protocol stability:

| ID | Title | Status | Key Invariants | Links |
|----|-------|--------|---|---|
| **DEC-020a** | Stream Concurrency, Cancellation, Backpressure | ✅ Designed | 128 max concurrent streams, deterministic cancellation | Implements: D5 transport layer |
| **DEC-022a** | Protocol Stability and Drift Detection | ✅ Accepted (D7) | Frame format frozen for V0.5, compile-time checks | Ensures: wire immutability |

### HA/DR Layer (F1–F7)

High-availability, disaster recovery, backup, and restore orchestration:

| ID | Title | Status | Key Invariants | Links |
|----|-------|--------|---|---|
| **DEC-019** | F1 — WAL Shipping Runtime Model | ✅ Designed | Segment shipping, LSN correlation, fencing events | Prerequisite for: F3, F4, F6 |
| **DEC-020b** | F3 — Quorum Runtime Sequencing | ✅ Designed + Implemented | Replica membership, fencing, promotion voting (18+ tests) | Depends on: DEC-019 (F1) |
| **DEC-024a** | F4 — Promotion and Failover Eligibility Boundary | ✅ Designed + Implemented | Eligibility criteria, no automatic failover (30+ tests) | Depends on: DEC-020b (F3) |
| **DEC-024b** | F2 — HA/DR Stream Mapping Over QUIC | ✅ Designed | Deterministic stream ID allocation, heartbeat/WAL/voting streams | Integration: Wave 18 |
| **DEC-025** | F6 — Restore and PITR Execution Plan | ✅ Designed + Implemented | Restore orchestration, PITR planning, audit binding (15+ tests) | Depends on: DEC-031 (F5) |
| **DEC-031** | F5 — Backup Physical Plan | ✅ Designed + Implemented | Deterministic backup planning, LSN binding, crash safety | Prerequisite for: F6 |
| **DEC-027** | F7 — HA/DR and Backup Audit Event Taxonomy | ✅ Designed + Implemented | Six event families, immutable trace binding, forensic accuracy | Observes: F1–F6 decisions |

### Catalog/Mutation Layer (E1–E7)

Catalog definition, procedure lifecycle, and SRPL integration:

| ID | Title | Status | Key Invariants | Links |
|----|-------|--------|---|---|
| **DEC-022b** | E2 — Alter Procedure Lifecycle Semantics | ✅ Designed (deferred impl) | Contract replacement, version binding, no ad hoc SQL | Prerequisite for: E7 |
| **DEC-023** | E3 — Drop Procedure Lifecycle Semantics | ✅ Designed (deferred impl) | Active→inactive transition, dependency validation | Prerequisite for: E7 |
| **DEC-029** | E4 — Catalog WAL Record Design | ✅ Designed + Implemented | Six catalog record types, durability semantics, replay safety | Implements: catalog durability |
| **DEC-030** | E7 — SRPL-DefinitionBatch Integration | ✅ Designed (impl pending) | SRPL→IR→DefinitionBatch pipeline, alter/drop support | Depends on: DEC-022b, DEC-023, DEC-029 |

### Protocol/Protobuf Layer

| ID | Title | Status | Key Invariants | Links |
|----|-------|--------|---|---|
| **DEC-021** | Protobuf Schema Contract for Frame/Result Compatibility | ✅ Accepted | Schema versioning policy, metadata contract, error mapping | Governs: proto evolution |

### Transaction/Storage Layer

| ID | Title | Status | Key Invariants | Links |
|----|-------|--------|---|---|
| **DEC-016** | V0.5 Runtime Plan Cache Scope | ✅ Accepted | Scaffold-only (no runtime cache yet), validation of key identity | Defers: cache implementation |
| **DEC-015** | Inventory Business Procedure Slice | ✅ Accepted | Typed invocation, deterministic mutation, local prototype | Validates: domain semantics |
| **DEC-032** | Storage Durable Page Format Baseline | ✅ Accepted | Single-source module ownership, page/LSN/WAL contracts | Freezes: storage format policy |

### Observability/Audit Layer

| ID | Title | Status | Key Invariants | Links |
|----|-------|--------|---|---|
| **DEC-028** | C4 — Admission Events Architecture | ✅ Approved | Immutable principal binding, no optional fields, forensic accuracy | Implements: admission audit |

### Release/Governance Layer

| ID | Title | Status | Key Invariants | Links |
|----|-------|--------|---|---|
| **DEC-026** | Release Gates, Blocker Taxonomy, and V0.5 Deferral Policy | ✅ Accepted (G4) | Measurable gates, explicit blockers, documented deferrals | Defines: V0.5 release criteria |

---

## Organization by Subsystem

### Core Foundation (Architecture)

Essential baseline decisions that all other layers depend on:

- **DEC-011:** Rust Workspace Topology (foundation)
- **DEC-012:** Phase 0 Contract Baselines (all crates)
- **DEC-013:** Local Vertical Prototype (proof of concept)
- **DEC-014:** Rust Crate Module Structure (code organization)
- **DEC-032:** Storage Durable Page Format Baseline (storage contracts)

### QUIC Transport & Identity (D1–D5)

Transport layer: QUIC runtime selection, frame contracts, stream lifecycle, identity extraction:

- **DEC-017:** QUIC Runtime Dependency (quinn + rustls selection)
- **DEC-018:** mTLS Identity Extraction (D3, certificate handling)
- **DEC-020a:** Stream Concurrency, Cancellation, Backpressure (D5 transport)
- **DEC-022a:** Protocol Stability and Drift Detection (D7, immutability)

### HA/DR System (F1–F7)

Replication, failover, backup, restore, and observability:

**Replication & Quorum:**
- **DEC-019:** F1 — WAL Shipping Runtime Model (primary→replica sync)
- **DEC-020b:** F3 — Quorum Runtime Sequencing (membership, voting)
- **DEC-024a:** F4 — Promotion and Failover Eligibility Boundary (criteria)
- **DEC-024b:** F2 — HA/DR Stream Mapping Over QUIC (multiplexing)

**Backup & Restore:**
- **DEC-031:** F5 — Backup Physical Plan (deterministic backup execution)
- **DEC-025:** F6 — Restore and PITR Execution Plan (recovery orchestration)

**Observability:**
- **DEC-027:** F7 — HA/DR and Backup Audit Event Taxonomy (forensic events)

### Catalog & Procedure Mutation (E1–E7)

Catalog metadata, procedure lifecycle, and SRPL compilation:

**Lifecycle Semantics:**
- **DEC-022b:** E2 — Alter Procedure Lifecycle Semantics (contract replacement)
- **DEC-023:** E3 — Drop Procedure Lifecycle Semantics (removal policy)

**Durability & Compilation:**
- **DEC-029:** E4 — Catalog WAL Record Design (durable mutations)
- **DEC-030:** E7 — SRPL-DefinitionBatch Integration (compiler pipeline)

### Plan Execution & Cache (E5–E6)

Execution planning and result caching:

- **DEC-016:** V0.5 Runtime Plan Cache Scope (scaffold only for V0.5)
- **DEC-015:** Inventory Business Procedure Slice (business logic validation)

### Protocol Contracts & Observability

- **DEC-021:** Protobuf Schema Contract (versioning policy)
- **DEC-028:** C4 — Admission Events Architecture (audit events)

### Release Governance

- **DEC-026:** Release Gates, Blocker Taxonomy, and V0.5 Deferral Policy (V0.5 criteria)

---

## Cross-References Verified

### Reference Summary

✅ **All cross-references validated (Wave 17, DEC-CLEAN-005)**

- **Valid references:** 127+
- **Missing targets:** 0
- **Broken references:** 0
- **Orphaned DECs:** 0

See `REFERENCE_AUDIT_REPORT.md` for full audit details.

### Key Dependency Chains

**F-Layer Chain (HA/DR):**
```
DEC-019 (F1: WAL)
  → DEC-018 (D3: mTLS)
  → DEC-020b (F3: Quorum)
    → DEC-024a (F4: Promotion Boundary)
      → DEC-025 (F6: Restore)
        ↓ depends on ↓
      → DEC-031 (F5: Backup)
    → DEC-027 (F7: Audit)
```

**E-Layer Chain (Catalog):**
```
DEC-022b (E2: Alter)
DEC-023 (E3: Drop)
  → DEC-029 (E4: Catalog WAL)
    → DEC-030 (E7: SRPL Integration)
```

**D-Layer Chain (Transport):**
```
DEC-017 (D1: QUIC Runtime)
  → DEC-018 (D3: mTLS)
  → DEC-020a (D5: Stream Concurrency)
    → DEC-024b (F2: Stream Mapping)
  → DEC-022a (D7: Protocol Stability)
```

---

## Status Legend

| Status | Meaning |
|--------|---------|
| ✅ **Accepted** | Decision locked, can be referenced by other work |
| ✅ **Approved (Dx)** | Approved for specific delivery phase Dx |
| ✅ **Designed** | Design specification complete; implementation may be deferred |
| ✅ **Designed + Implemented** | Specification and implementation complete |
| 🟡 **In Progress** | Implementation underway (e.g., DEC-026 gates) |

---

## Finding Duplicates with Explanation

The following DEC numbers appear twice with **distinct, non-overlapping scopes**. This is intentional governance:

### DEC-020 Pair

**DEC-020a:** `DEC-020-stream-concurrency.md`
- **Layer:** D5 (Transport)
- **Scope:** QUIC stream lifecycle, concurrency limits, backpressure protocol
- **Audience:** D4 executor, F1 shipping runtime, connection handlers

**DEC-020b:** `DEC-020-quorum-runtime.md`
- **Layer:** F3 (Replication)
- **Scope:** Replica membership, write admission, promotion eligibility
- **Audience:** F1 shipping, quorum decisions, failover orchestration

**Rationale:** Both decisions are integral to HA/DR but concern different layers (transport vs. consensus). Suffix notation (a/b) disambiguates while preserving the unified numbering for related architecturally adjacent concerns.

---

### DEC-022 Pair

**DEC-022a:** `DEC-022-protocol-stability.md`
- **Layer:** D7 (Protocol)
- **Scope:** Wire format immutability, drift detection, compile-time checks
- **Audience:** Transport, protocol engineers, CI/CD

**DEC-022b:** `DEC-022-alter-procedure-lifecycle.md`
- **Layer:** E2 (Catalog)
- **Scope:** Alter Procedure semantics, contract replacement, catalog lifecycle
- **Audience:** Catalog, compilation, mutation operators

**Rationale:** Protocol stability and procedure lifecycle are separate concerns (wire format vs. catalog mutations) even though both affect V0.5 definition stability. Suffix notation prevents renumbering cascades.

---

### DEC-024 Pair

**DEC-024a:** `DEC-024-promotion-failover-boundary.md`
- **Layer:** F4 (Orchestration)
- **Scope:** Promotion eligibility criteria, failover boundaries, no automatic failover
- **Audience:** Promotion voting, quorum decisions, failover orchestration

**DEC-024b:** `DEC-024-hadr-stream-mapping.md`
- **Layer:** F2 (Transport Multiplexing)
- **Scope:** Deterministic stream ID allocation, WAL/heartbeat/voting stream separation
- **Audience:** Stream transport, F1 shipping, F3 quorum signaling

**Rationale:** Promotion eligibility (orchestration) and stream mapping (transport multiplexing) are distinct architectural decisions affecting different subsystems. Suffix notation clarifies the separation.

---

## Wave 17 Deliverables

This index is created as part of **Wave 17 (DEC-CLEAN-005)** audit:

- [x] **REFERENCE_AUDIT_REPORT.md** — Comprehensive audit with validation status
- [x] **INDEX.md** (this file) — Master index organized by layer and subsystem
- [x] **decision_reference_consistency.rs** — Automated test for CI/CD validation

---

## How to Use This Index

### For Architects

Find decisions by **layer** (D1–D7, F1–F7, E1–E7):
- Top of document organized by stack
- Browse subsystem sections for deep dependencies

### For Implementation Teams

Find decisions by **subsystem**:
- "Organization by Subsystem" section
- Links show implementation status
- See "Status Legend" for completion stage

### For Auditors

Find decisions by **governance**:
- "Cross-References Verified" section
- Reference chains show dependency ordering
- See `REFERENCE_AUDIT_REPORT.md` for full audit

### For Release Planning

Find decisions by **release gate**:
- See DEC-026 (Release Gates V0.5)
- Index shows implementation status (✅ Implemented vs. 🟡 Deferred)

---

## Contributing

When adding a new DEC:

1. Use next available number (currently 033+)
2. Include "Status" section at top
3. Cite related DECs in "References:" field
4. Add entry to this index under appropriate subsystem/layer
5. Run `cargo test decision_reference_consistency` to validate

When modifying an existing DEC:

1. Update "Status" if completion stage changes
2. Add new "References:" if newly dependent on other DECs
3. Ensure no circular dependencies (use audit report to verify)
4. Run `cargo test decision_reference_consistency` after changes

---

## Maintenance Schedule

- **Wave 17 (Current):** DEC-CLEAN-005 audit, index creation
- **Wave 18:** Implementation of deferred decisions (E2, E3, E7, etc.)
- **Wave 19:** Archive completed decisions to `archived/` subdirectory
- **Post-V1.0:** Consolidate decision history into immutable archive

---

**Last Audit:** Wave 17 (2026-01-XX)  
**Audit Agent:** Risk and Decision Manager (DEC-CLEAN-005)  
**Next Review:** Wave 19 (post-V0.5 completion)
