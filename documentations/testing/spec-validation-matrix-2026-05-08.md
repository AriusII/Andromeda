# Specification Validation Matrix - 2026-05-08

## Purpose

Map each Andromeda v0 specification to the owner validation surface that should
produce release evidence before the specification is used as implementation or
release proof.

This matrix is a Step 12 documentation trace artifact. It connects
`documentations/specs/*` contracts to Step 11 owner-suite labels and release
gaps. It does not run tests and does not claim that any gate passed.

## Scope

This matrix covers the v0 specifications listed by
`documentations/specs/index.md` on 2026-05-08.

The matrix records:

- the specification path;
- the status class from the specification index;
- the primary owner-suite area that should produce retained evidence;
- the minimum release-evidence gap to close before stronger claims are made.

## Non-goals

- Do not move executable tests into `documentations/testing`.
- Do not rename specifications or crates.
- Do not claim release readiness from this matrix.
- Do not replace `step-11-validation-matrix.md`, release evidence records, ADRs,
  DECs, or owner-crate test suites.
- Do not treat spec text, fuzz compile checks, benchmark output, RAM state,
  GPU output, audit output, or temporary output as database truth.

## Prerequisites

Before promoting any row from planning evidence to release evidence, collect:

1. The exact branch and commit.
2. `git status --short`.
3. The exact command or workflow name.
4. Toolchain versions.
5. Pass, fail, skipped, or partial status.
6. Retained artifact path.
7. Residual risk and reviewer disposition.

For C4/C5 paths, add crash/recovery, security, protocol, audit, fuzz, Miri,
Loom, property, or supply-chain evidence as required by the owning subsystem.

## Procedure

1. Find the specification in the matrix.
2. Use the owner-suite mapping to locate the first validation surface.
3. Use `step-11-validation-matrix.md` for exact owner commands and combined
   C5 gate composition.
4. Record retained evidence with `release-evidence-template.md`.
5. Keep any unrun, partial, smoke-only, or continue-on-error result as residual
   risk.

## Status Legend

| Status | Meaning for validation |
|---|---|
| Implemented evidence, not release approval | Current code or tests cover a bounded behavior. Retained release evidence is still required. |
| Partial implementation | Some code, tests, or contracts exist. Integration, durability, recovery, or end-to-end proof remains open. |
| Contract target | The document defines a target contract. Owner tests and implementation evidence must be added before runtime claims. |
| Planned gap | The document records future work or a blocked area. Treat it as planning evidence only. |

## Spec-To-Test Matrix

| Specification | Index status | Primary owner-suite mapping | Release evidence gap |
|---|---|---|---|
| [AuditLedger v0](../specs/AuditLedger_v0.md) | Partial implementation | `tests/security`, RPC/security/audit gate, `andromeda-observe` durable audit tests, and `andromeda-exec` audit completion evidence. | Retain evidence that accepted and rejected paths emit required audit evidence without making audit records database truth or an Admin RPC audit query claim. |
| [BackupManifest v0](../specs/BackupManifest_v0.md) | Implemented evidence, not release approval | `tests/storage` and `tests/recovery`, especially backup physical plan, backup execution plan, restore contract, and WAL archive coverage. | Retain full backup, restore, WAL archive, PITR target, compatibility, and preflight evidence before production backup claims. |
| [BufferPoolPolicy v0](../specs/BufferPoolPolicy_v0.md) | Partial implementation | `tests/storage` and C5 storage/recovery gate, including WAL durability fence and disk-manager crash-safety tests. | Add retained checkpoint scheduler, read-ahead, eviction, and visible-commit owner evidence where release scope includes those paths. |
| [CatalogDiff v0](../specs/CatalogDiff_v0.md) | Contract target with partial evidence | `tests/srpl`, catalog compatibility tests, DefinitionBatch tests, and catalog publication recovery evidence. | Prove concrete `CatalogDiff` construction, compatibility classification, plan invalidation, and complete-sequence catalog publication before implementation claims. |
| [CatalogObjectModel v0](../specs/CatalogObjectModel_v0.md) | Contract target | `tests/srpl`, `tests/recovery`, catalog store contract tests, DefinitionBatch compatibility, and catalog WAL bridge tests. | Retain catalog object identity, version history, dependency graph, WAL publication, and recovery evidence before treating catalog truth as complete. |
| [ContractCompatibility v0](../specs/ContractCompatibility_v0.md) | Partial implementation | `tests/security` for ContractHash owner evidence and catalog/SRPL compatibility tests. | Retain transition coverage for exact hash, additive-only, rejected changes, diagnostics, and catalog diff integration. |
| [ContractHash Canonicalization v0](../specs/ContractHash_Canonicalization_v0.md) | Implemented evidence, not release approval | `tests/security` and contract owner suites, including ContractHash golden vectors and facade compatibility checks. | Retain golden evidence and migration policy before claiming stable cross-release hashing for every Procedure and catalog object shape. |
| [DatabaseManifest v0](../specs/DatabaseManifest_v0.md) | Contract target with current domain-model validation evidence | `tests/storage`, `tests/recovery`, manifest publication, layout publication, and recovery completeness tests. | Add durable manifest byte-codec, unknown-version rejection, root-switch, and replay evidence before release promotion. |
| [DecisionTrace v0](../specs/DecisionTrace_v0.md) | Partial implementation | `andromeda-observe` decision event tests plus optimizer, catalog, execution, and security owner suites that emit decisions. | Retain trace coverage for bounded fields, ignored evidence, final decisions, and correlation across Procedure, plan, WAL, audit, and recovery paths. |
| [DefinitionBatch v0](../specs/DefinitionBatch_v0.md) | Contract target with current catalog behavior and pending extraction | `tests/srpl`, `tests/recovery`, catalog store contract, catalog WAL bridge, and DefinitionBatch compatibility tests. | Add crash tests for begin, apply, commit, incomplete tail, replay rejection, and all-or-nothing publication before C5 catalog claims. |
| [DurableCommitEvidence v0](../specs/DurableCommitEvidence_v0.md) | Partial implementation | `tests/recovery`, C5 combined gate, `andromeda-tx` commit-log durability, WAL owner tests, and execution recovery visibility tests. | Retain end-to-end evidence that prepared state remains invisible until the terminal WAL record is within the durable WAL prefix. |
| [DurableRollbackEvidence v0](../specs/DurableRollbackEvidence_v0.md) | Partial implementation | `tests/recovery`, transaction rollback terminal evidence, WAL owner tests, and execution terminal outcome tests. | Prove rollback terminal evidence is durable completion evidence, not visible mutation evidence, and survives replay boundaries. |
| [FrameHeader RPC v0](../specs/FrameHeader_RPC_v0.md) | Implemented evidence, not release approval | `tests/rpc`, `andromeda-rpc-protocol` owner tests, QUIC codec and protocol stability tests, and malformed-frame fuzz evidence. | Retain runtime route/admission/audit evidence and sustained malformed-input fuzz before network release claims. |
| [GpuBatchPolicy v0](../specs/GpuBatchPolicy_v0.md) | Contract target and planned accelerator policy | GPU exclusion scans, topology tests, `tests/maps`, benchmark owner tests, and future optional GPU owner suites. | Prove CPU fallback, disablement, trace fields, and no GPU imports in commit, WAL, rollback, recovery, MVCC, catalog publication, or security-critical paths. |
| [HADR Quorum and Fencing v0](../specs/HadrQuorumFencing_v0.md) | Partial implementation | `tests/storage`, `tests/rpc`, HADR membership, quorum, promotion, WAL shipping, and HADR stream mapping tests. | Add retained local cluster simulation, fencing-under-partition proof, promotion eligibility evidence, and failover-with-recovery transcript. |
| [MapDescriptor v0](../specs/MapDescriptor_v0.md) | Contract target with partial catalog and WAL classification evidence | `tests/maps`, catalog statistics publication tests, and future Map runtime owner tests. | Prove Map descriptor ownership, refresh modes, active switch, rollback, rebuild, recovery value evidence, and non-authoritative Map output. |
| [MapRefreshValidation v0](../specs/MapRefreshValidation_v0.md) | Partial implementation | `tests/maps`, catalog statistics publication, optimizer MapLookup gates, and C5 recovery checks when publication is durable. | Retain refresh scheduler, durable Map storage, staleness gates, summarizability gates, and recovery evidence before release claims. |
| [MvccIsolation v0](../specs/MvccIsolation_v0.md) | Partial implementation | `tests/recovery`, transaction owner tests, MVCC isolation tests, and execution visibility gates. | Add retained long-reader, snapshot, write-conflict, rollback invisibility, and serializable-gap evidence before stronger isolation claims. |
| [PageHeader and PageTrailer v0](../specs/PageHeader_PageTrailer_v0.md) | Implemented evidence, not release approval | `tests/storage`, page codec property tests, page/trailer golden vectors, and storage fuzz targets. | Retain malformed-input, checksum, torn-write, cross-architecture, and recovery integration evidence for every promoted page-bearing artifact. |
| [PageLifecycle v0](../specs/PageLifecycle_v0.md) | Partial implementation | `tests/storage`, buffer-pool WAL fence tests, disk-manager crash safety, and recovery replay page record tests. | Add read-ahead, checkpoint, flush ordering, eviction, and visible-commit separation evidence before release promotion. |
| [PlanCacheKey v0](../specs/PlanCacheKey_v0.md) | Partial implementation | Catalog plan-cache tests, optimizer safety tests, and DecisionTrace evidence. | Retain invalidation, bounded PlanClass, stale evidence rejection, and advisory ScenarioEvidence ignored-when-unsafe tests. |
| [ProcedureContract v0](../specs/ProcedureContract_v0.md) | Implemented evidence, not release approval | `tests/security`, `tests/srpl`, contract owner tests, Procedure manifest projection, and execution admission tests. | Retain full propagation evidence from SRPL/catalog binding through admission, invocation, ResultStream, Procedure Store, audit, and recovery paths. |
| [ProcedureInvocationTrace v0](../specs/ProcedureInvocationTrace_v0.md) | Partial implementation | `andromeda-observe`, `andromeda-exec` runtime contract tests, Procedure Store tests, and audit completion tests. | Retain lifecycle order, bounded payload policy, rejection, terminal outcome, and replay correlation evidence. |
| [RecoveryReport v0](../specs/RecoveryReport_v0.md) | Contract target | `tests/recovery`, forensic startup tests, storage recovery contract, and recovery completeness tests. | Add retained FastStart, SafeStart, ForensicStart, anomaly classification, and report artifact evidence before operational claims. |
| [RecoveryTrace v0](../specs/RecoveryTrace_v0.md) | Partial implementation | `andromeda-observe` recovery event tests plus storage recovery and forensic startup owner tests. | Prove complete trace shape, bounded fields, recovery mode decisions, WAL corruption handling, and RecoveryReport ties. |
| [SecurityAdmission v0](../specs/SecurityAdmission_v0.md) | Partial implementation | `tests/security`, `andromeda-security-contract` library tests, QUIC route admission, execution IAM pipeline, and audit completion tests. | Retain fail-closed pre-transaction evidence for wrong surface, disabled principal, missing permission, malformed frame, and resource denial. |
| [SecurityAdmissionCanonicalOrder v0](../specs/SecurityAdmissionCanonicalOrder_v0.md) | Contract target | `tests/security`, route admission tests, execution IAM pipeline, and audit evidence tests. | Prove the canonical order from protocol identity through contract, surface, principal, permission, decision, audit, and transaction creation. |
| [SegmentIndex v0](../specs/SegmentIndex_v0.md) | Contract target with current segment descriptor and contiguity validation evidence | `tests/storage`, SegmentIndex decode fuzz target, manifest and cold snapshot recovery tests. | Retain durable byte-codec, contiguity, checksum, decode-before-allocate, unknown-version rejection, and recovery handling evidence. |
| [StatsObject v0](../specs/StatsObject_v0.md) | Contract target | Catalog statistics publication tests, optimizer tests, and DecisionTrace evidence. | Map existing statistics publication to the complete StatsObject shape before optimizer or release claims. |
| [StructuredObjectPayload v0](../specs/StructuredObjectPayload_v0.md) | Implemented evidence, not release approval | Protocol payload tests, generated validation tests, structured payload bounds, and ResultStream metadata tests. | Retain descriptor hash, row-count, payload bounds, absence policy, and physical payload truth-boundary evidence. |
| [SurfaceSeparation v0](../specs/SurfaceSeparation_v0.md) | Partial implementation | `tests/security`, `tests/rpc`, QUIC route admission, security-contract tests, and CLI/Admin surface checks. | Retain evidence that Application, Administration, Cluster, monitoring, and forensic surfaces reject cross-surface access before transaction creation. |
| [TypeSystem v0](../specs/TypeSystem_v0.md) | Partial implementation | Contract owner tests, SRPL binder/type tests, StructuredObject tests, and Procedure contract binding tests. | Retain compatibility and absence-policy evidence across SRPL, catalog, Procedure manifests, StructuredObject payloads, and ResultStream. |
| [WalRecord v0](../specs/WalRecord_v0.md) | Implemented evidence, not release approval | `tests/recovery`, `tests/storage`, `andromeda-wal` owner tests, WAL round-trip fuzz, and file WAL recovery tests. | Retain multi-segment archive, persisted ChainHash, truncation, checksum, wrong-generation, and recovery replay evidence. |
| [WAL Shipping v0](../specs/WalShipping_v0.md) | Partial implementation | `tests/storage`, `tests/rpc`, WAL shipping reclaimability, HADR membership, promotion, and stream mapping tests. | Retain QUIC streaming, retry scheduling, persistent multi-segment archive, durable ACK, backpressure, fencing, and replica catchup evidence. |

## Combined Release Gate Notes

Use `step-11-validation-matrix.md` as the command source for these combined
areas:

| Area | When to add the combined gate |
|---|---|
| C5 durable Procedure execution | Any claim crosses Procedure admission, transaction, WAL, heap/page state, recovery visibility, ResultStream, Procedure Store, or durable audit. |
| Protocol and security | Any claim crosses RPC frame parsing, QUIC routing, surface separation, IAM admission, authorization, audit evidence, or malformed-frame rejection. |
| Storage and recovery | Any claim touches WAL, page, heap, B-Tree, manifest, backup, restore, PITR, HA/DR, buffer-pool flushing, or replay. |
| Adaptive and Maps | Any claim touches statistics, optimizer, PlanCache, Maps, ScenarioEvidence, benchmark evidence, or optional GPU acceleration. |

## Validation

This documentation-only matrix should be validated by targeted checks:

```powershell
rg -n "spec-validation-matrix-2026-05-08|Spec-to-test|Specification Validation Matrix" documentations -S
git diff --check -- documentations/testing/spec-validation-matrix-2026-05-08.md documentations/testing/index.md documentations/specs/index.md
git diff -- documentations/testing/spec-validation-matrix-2026-05-08.md documentations/testing/index.md documentations/specs/index.md
```

Rust workspace gates are required only when an implementation packet claims a
linked owner-suite result.

## Troubleshooting

| Symptom | Corrective action |
|---|---|
| A specification is cited as release proof. | Require retained owner-suite evidence and release-owner disposition. |
| A row lists a test area but no command was run. | Record the row as mapping evidence only and link the unrun gate as residual risk. |
| A C5 spec has owner tests but no crash/recovery evidence. | Keep the claim partial until crash/recovery and visibility evidence is retained. |
| A benchmark, GPU, Map, audit, or trace artifact is described as truth. | Reword it as advisory, forensic, or post-fact evidence and cite durable WAL plus recovery truth boundaries. |

## References

- [Specification Index](../specs/index.md)
- [Testing Documentation Index](index.md)
- [Step 11 Validation Matrix](step-11-validation-matrix.md)
- [Release Evidence Template](release-evidence-template.md)
- [CI Release Gate Evidence](ci-release-gate-evidence.md)
- [Step 12 Documentation Closure](../governance/step-12-documentation-closure-2026-05-08.md)
- [ADR Backlog 2026-05-08](../governance/adr-backlog-2026-05-08.md)
- `AGENTS.md`
