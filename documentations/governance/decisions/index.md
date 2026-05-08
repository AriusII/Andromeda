# Andromeda architecture decision records (ADRs)

This index tracks the active ADR corpus for Andromeda SGBDRT.

- **Latest accepted decision:** DEC-041
- **Latest numbered record in directory:** DEC-041

---

## Release gates

### Master gate index

- **DEC-035: Release gate chain** (authoritative gate contract for release readiness)

### Release approval

- **DEC-036: Release readiness approval** (formal sign-off record against DEC-035)

---

## Index by milestone

### Foundation and early vertical slice

- DEC-011: Rust workspace topology
- DEC-012: Phase 0 contract baselines
- DEC-013: Local vertical prototype
- DEC-014: Rust crate module structure
- DEC-015: Inventory business procedure slice
- DEC-016: Plan cache scope
- DEC-017: QUIC runtime dependency
- DEC-018: mTLS identity extraction
- DEC-019: F1 WAL shipping runtime
- DEC-020: F3 quorum runtime
- DEC-020b: Stream concurrency
- DEC-021: Protobuf schema contract
- DEC-022: Alter procedure lifecycle (E2)
- DEC-022b: Protocol stability
- DEC-023: Drop procedure lifecycle (E3)
- DEC-024: Promotion and failover boundary (F4)
- DEC-024b: HA/DR stream mapping
- DEC-025: Restore orchestration (F6)
- DEC-026: Release gates and deferral policy
- DEC-027: HA/DR and backup audit taxonomy (F7)
- DEC-028: Admission audit events (C4)
- DEC-029: Catalog WAL (E4)
- DEC-030: SRPL-DefinitionBatch integration (E7)
- DEC-031: Backup physical plan (F5)
- DEC-032: Storage format gate

### Durability milestone

- DEC-033: Durable audit ledger
- DEC-034: V1.0 readiness assessment

### Implementation batch

- DEC-037: Risk register updates
- DEC-038: B-Tree mutations deferred

### Implementation-to-release bridge

- DEC-039: Optimizer intermediate pass contract *(PROPOSED)*

### Lot 5 boundary governance

- DEC-040: RPC, QUIC, and security boundary governance for Lot 5.1 *(ACCEPTED)*
- DEC-041: Security contract boundary for Lot 5.4A *(ACCEPTED)*

### Lot 5 documentation acceptance references

- ADR-0012: QUIC/RPC no-gRPC documentation acceptance
- `documentations/specs/FrameHeader_RPC_v0.md`: accepted RPC frame header documentation contract
- `documentations/specs/SecurityAdmission_v0.md`: accepted pre-transaction security admission documentation contract
- `documentations/specs/AuditLedger_v0.md`: accepted durable audit ledger documentation contract

### Release gate cycle

- DEC-035: Release gate chain *(ACCEPTED)*
- DEC-036: Release readiness approval *(ACCEPTED)*

### Current release governance artifacts

- `documentations/governance/risk-register-2026-05-08.md`: current blocker and risk register for the 2026-05-08 review snapshot.
- `documentations/governance/release-readiness-gates-2026-05-08.md`: current release-readiness gate checklist for the 2026-05-08 review snapshot.

These artifacts are not decision records and do not change the latest accepted
decision. They record current blockers and gate criteria. DEC-035, DEC-036, and
DEC-037 remain indexed governance context, but their future dates mean they
cannot serve as current release proof for the 2026-05-08 review snapshot.

---

## Index by workstream

### Storage and recovery

- DEC-019, DEC-024, DEC-024b, DEC-025, DEC-031, DEC-032, DEC-035, DEC-036, DEC-038

### Transaction and execution

- DEC-016, DEC-020, DEC-020b, DEC-026, DEC-028, DEC-035, DEC-036, DEC-039

### Compiler and catalog

- DEC-022, DEC-023, DEC-029, DEC-030, DEC-039

### Security, protocol, and observability

- DEC-017, DEC-018, DEC-021, DEC-027, DEC-033, DEC-035, DEC-036, DEC-037, DEC-040, DEC-041

---

## Status legend

| Status | Meaning |
|---|---|
| ACCEPTED | Approved and active for implementation/governance |
| PROPOSED | Under review; not yet locked |
| DEPRECATED | Superseded by a newer decision |
| REJECTED | Evaluated and declined |

---

## Implementation-To-Release Cross-Reference Map

| Area | Implementation batch records | Release gate cycle records |
|---|---|---|
| Risk and governance | DEC-037 | DEC-035, DEC-036 |
| Storage boundaries | DEC-038 | DEC-035, DEC-036 |
| Optimizer contract | DEC-039 *(proposed)* | DEC-035 gate alignment |
| Release authority | Implementation handoff context | DEC-036 sign-off |
| Lot 5 RPC and security boundary | DEC-040, DEC-041, ADR-0012, `FrameHeader_RPC_v0`, `SecurityAdmission_v0`, `AuditLedger_v0` | DEC-035 gate alignment |
| 2026-05-08 current release governance | `risk-register-2026-05-08`, `release-readiness-gates-2026-05-08` | Current blocker inventory; not release approval |

---

## Last updated

- **Date:** 2026-05-08
- **Next review:** At the next release gate checkpoint
