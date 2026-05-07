# DEC-036: release gate cycle Release Approval

**Status:** ACCEPTED ✓  
**Date:** 2026-06-16  
**Authority:** Release Governance Board (Architecture, Recovery, Security, Observability, Storage, Execution)

---

## Executive summary

This decision records final release gate cycle sign-off using the locked release contract in DEC-035. Approval is contingent on the seven blocking gates, eight audit families, locked decision corpus, and verification test matrix. This document is the authoritative release approval trail for release gate cycle governance.

---

## Context

release gate cycle closes the release-governance pass after implementation-to-release gate hardening. The project requires explicit sign-off records rather than implicit acceptance. Approval must remain auditable, reproducible, and tied to bounded gate evidence.

---

## Decision

release gate cycle is approved **when and only when** all checklist items in this record are marked PASS with evidence links. Any BLOCKED item reverts status to “Not Approved”.

---

## 1) Release sign-off checklist

### A. Seven blocking gates

| Gate | Status | Evidence |
|---|---|---|
| G1 Recovery failure | ✅ PASS | DEC-035 §1, §8; storage recovery suites |
| G2 Visibility violation | ✅ PASS | DEC-035 §1, §3, §9; exec/tx visibility suites |
| G3 ContractHash mismatch acceptance | ✅ PASS | DEC-035 §1, §5; pre-transaction contract rejection tests |
| G4 Audit omission | ✅ PASS | DEC-035 §1, §2; observe durable audit + family contracts |
| G5 Panic in critical path | ✅ PASS | DEC-035 §1, §6; critical-path scan policy + typed error model |
| G6 Silent corruption | ✅ PASS | DEC-035 §1, §7; PublishedColdSegment immutability checks |
| G7 Non-reproducible crash | ✅ PASS | DEC-035 §1, §8; crash matrix + recovery replay properties |

### B. Eight audit families completeness

| Family | Status | Evidence |
|---|---|---|
| SecurityAuditTrace | ✅ PASS | `andromeda-observe` schema + validation |
| CatalogChangeTrace | ✅ PASS | `CatalogMutationTrace` mapping + durable family mapping |
| AdminOperationTrace | ✅ PASS | schema + permission/surface validation |
| ProcedureInvocationTrace | ✅ PASS | exec invocation trace lifecycle emission |
| TransactionTrace | ✅ PASS | tx transition evidence and trace integration |
| PlanDecisionTrace | ✅ PASS | decision trace family and query semantics |
| RecoveryTrace | ✅ PASS | recovery startup/boundary evidence |
| ClusterEventTrace | ✅ PASS | HA/DR audit taxonomy lineage (DEC-033/DEC-027) |

### C. Governance and delivery checklist

| Item | Status | Notes |
|---|---|---|
| All implementation-to-release todos complete (target ledger: 404) | ✅ PASS | Governance declaration locked at release sign-off; unresolved items block this row. |
| All gate decisions locked | ✅ PASS | DEC-035 + prior DEC chain (026/033/037/038/039) |
| Verification tests passing | ✅ PASS | Gate suites and workspace gate commands recorded in DEC-035 |
| Code committed and traceable | ✅ PASS | Release approval requires clean release commit lineage |
| Decision corpus updated | ✅ PASS | DEC-035 and DEC-036 registered in decision index |

---

## 2) Sign-off authority trail

| Role | Responsibility | Sign-off status |
|---|---|---|
| Chief Architect | Cross-engine coherence, doctrine compliance | ✅ Approved |
| Release Governance | Gate adjudication and blocker policy | ✅ Approved |
| WAL/Recovery | Durability and crash convergence proof | ✅ Approved |
| Transaction Kernel | Visibility and state-machine correctness | ✅ Approved |
| Security/IAM | Auth/audit omission prevention | ✅ Approved |
| Observability | Trace family completeness and queryability | ✅ Approved |
| Storage Engine | Cold immutability and corruption boundaries | ✅ Approved |
| Execution Runtime | Pre-transaction gates and completion semantics | ✅ Approved |

---

## 3) Verification matrix summary (release gate cycle)

The following minimum suite is required for release approval (15+ scenarios):

1. `crates/andromeda-exec/tests/recovery_visibility_gates.rs`
2. `crates/andromeda-exec/tests/core_io_gates.rs`
3. `crates/andromeda-exec/tests/v0_vertical_e2e.rs`
4. `crates/andromeda-exec/tests/integration_execution_path.rs`
5. `crates/andromeda-storage/tests/crash_recovery_impl.rs` (40 scenarios)
6. `crates/andromeda-storage/tests/wal_scan_recovery_contract.rs`
7. `crates/andromeda-storage/tests/wal_durability_fence_contract.rs`
8. `crates/andromeda-storage/tests/property_recovery_replay.rs`
9. `crates/andromeda-storage/tests/disk_manager_durability_crash_safety.rs`
10. `crates/andromeda-storage/tests/core_io_gates.rs`
11. `crates/andromeda-storage/tests/layout_publication_contract.rs`
12. `crates/andromeda-tx/tests/commit_log_durability.rs`
13. `crates/andromeda-tx/tests/commit_log_gates_final.rs`
14. `crates/andromeda-tx/tests/mvcc_gc_durability_contract.rs`
15. `crates/andromeda-observe/tests/audit_family_contract.rs`
16. `crates/andromeda-observe/tests/durable_audit_sink_contract.rs`
17. `crates/andromeda-exec/tests/c6_recovery_security_audit.rs`

---

## 4) Release stop conditions

Any of the following immediately revokes approval:

1. Recovery replay divergence for same WAL + manifest inputs.
2. ContractHash mismatch accepted past admission.
3. Missing audit evidence for security/admin/catalog/recovery boundary events.
4. Runtime panic introduced in admission/auth/commit/WAL/recovery critical paths.
5. Mutation accepted on published cold storage segments.
6. Visibility mismatch that violates durable commit ordering.

---

## 5) Rollback or disablement plan

If approval is revoked:

1. Tag release candidate as blocked.
2. Open blocker record referencing failing gate ID.
3. Revert or hotfix only through typed contract path.
4. Re-run affected gate tests and full workspace validation.
5. Re-issue approval by updating this decision record revision.

---

## Related records

- `documentations/governance/decisions/DEC-035-release-gate-chain.md`
- `documentations/governance/decisions/DEC-033-durable-audit-ledger.md`
- `documentations/governance/decisions/DEC-026-release-gates-and-deferral-policy.md`
- `documentations/governance/decisions/DEC-037-risk-register-updates.md`
