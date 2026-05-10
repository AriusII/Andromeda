# Specifications

> **Status:** Normative V0 specification index
> **Scope:** `docs/specifications/`

## Purpose

This folder contains Andromeda's normative V0 technical specifications. Source code, tests, ADRs,
and roadmap work packets should link here rather than to legacy `docs/specs` paths.

Each specification is an implementation gate, not a documentation placeholder. A spec can unblock
owner implementation only when it states source grounding, criticality, rejection behavior,
serialization boundaries, owner crates, acceptance evidence, tests, and the retained evidence
required for the highest C0-C5 path it touches.

## Specification groups

| Area | Specifications |
|---|---|
| Type, SRPL, and contracts | `SPEC_TYPE_SYSTEM_V0.md`, `SPEC_PROCEDURE_CONTRACT_V0.md`, `SPEC_SRPL_GRAMMAR_V0.md`, `SPEC_SRPL_BINDER_V0.md`, `SPEC_SEMANTIC_IR_V0.md` |
| Catalog and DefinitionBatch | `SPEC_CATALOG_OBJECT_MODEL_V0.md`, `SPEC_DEFINITION_BATCH_V0.md` |
| Storage and recovery | `SPEC_TRANSACTION_STATE_MACHINE_V0.md`, `SPEC_WAL_RECORD_V0.md`, `SPEC_FILE_WAL_SEGMENT_V0.md`, `SPEC_PAGE_FORMAT_V0.md`, `SPEC_DATABASE_MANIFEST_V0.md`, `SPEC_SEGMENT_INDEX_V0.md`, `SPEC_RECOVERY_REPORT_V0.md`, `SPEC_CRASH_RECOVERY_TEST_PLAN_V0.md` |
| RPC, security, and audit | `SPEC_RPC_FRAME_V0.md`, `SPEC_RESULT_STREAM_V0.md`, `SPEC_SECURITY_ADMISSION_V0.md`, `SPEC_AUDIT_LEDGER_V0.md`, `SPEC_DECISION_TRACE_V0.md` |
| Optimizer, statistics, and hardware | `SPEC_PLAN_CACHE_KEY_V0.md`, `SPEC_STATS_OBJECT_V0.md`, `SPEC_SCENARIO_EVIDENCE_V0.md`, `SPEC_GPU_EXECUTION_POLICY_V0.md`, `SPEC_ADVISORY_OPTIMIZER_HARDWARE_V0.md` |
| Operations | `SPEC_HADR_CLUSTER_MANIFEST_V0.md` |

## Change rule

Any change to a persisted format, RPC-visible frame, security boundary, catalog lifecycle, recovery
behavior, or advisory evidence promotion rule must update the relevant `SPEC_*_V0.md` file and any
test that asserts its decision coverage.

## Closure rule

P01 baseline closure is checked by `python -B tools/testing/p01_spec_baseline_check.py --strict`.
For new specifications or material revisions after this addendum, closure also requires all of the
following:

- The spec is listed in this index and uses the current `docs/templates/SPEC_TEMPLATE.md` section
  model, including source grounding and criticality/evidence classification for the revised scope.
- Every persisted, network-visible, security-critical, recovery-critical, or semantic IR structure
  has an explicit owner, canonical representation, and rejection behavior.
- C4/C5 paths name the doctrine invariant they affect and identify retained evidence, not only a
  green build, scaffold, demo, benchmark, trace, or simulation-only drill.
- Durable state, external surfaces, security, admission, audit, backup, restore, HA/DR, WAL, MVCC,
  or catalog-truth specs define the crash/recovery or fail-closed evidence required for acceptance.
- The acceptance summary includes literal `Owner`, `Evidence`, and `Reject` entries tying the
  changed path to criticality, source artifact, validation command or review record, retained
  evidence, and the claim that must be rejected when proof is missing.

Do not claim production readiness or release readiness from this index. Those claims require the
separate release evidence packet and roadmap release gates.

## Terminology rule

Terms such as `query`, `view`, `dynamic SQL`, `null ambient`, and `native-layout serialization` may
appear only when explicitly rejected, disambiguated, or mapped to canonical Andromeda terms. They must
not define a native application capability.
