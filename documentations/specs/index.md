# Andromeda Specification Index

## Purpose

Provide the entry point for Andromeda v0 specifications in `documentations/specs`.
Use this page to find the owning contract, understand whether the document is a
target, a partial implementation description, or backed by executable evidence,
and locate related implementation ledgers and runbooks.

## Scope

This index covers the specification documents in this directory. It also links
to implementation ledgers, architecture references, operations runbooks, and
release-evidence documents that are needed to interpret each specification
honestly.

## Non-goals

This index does not approve release readiness, create new runtime behavior,
replace architecture decision records, or turn a v0 documentation contract into
proof that every runtime path is implemented.

## Prerequisites

Before using a specification as implementation evidence, review:

- [Andromeda Documentation](../README.md) for documentation area ownership.
- [Architecture Index](../architecture/index.md) for doctrine and decision
  references.
- [Implementation Index](../implementation/index.md) for current reality
  ledgers and residual risks.
- [Operations Runbook Index](../operations/runbooks/index.md) for operational
  status terms.
- [Step 11 Validation Matrix](../testing/step-11-validation-matrix.md) for
  release-gate validation context.
- [Specification Validation Matrix - 2026-05-08](../testing/spec-validation-matrix-2026-05-08.md)
  for spec-to-test owner-suite mapping.

## Procedure

1. Use the status legend before reading the specification register.
2. Open the specification for the contract details, offsets, ordering rules, or
   validation matrix.
3. Cross-check any implemented claim against the implementation ledgers.
4. Use runbooks for operator procedures and incident drills.
5. Record exact validation commands and results before promoting a contract to
   release evidence.

## Status legend

| Status | Meaning |
| --- | --- |
| Implemented evidence, not release approval | The specification names current code or tests for a bounded behavior, but candidate release gates still apply. |
| Partial implementation | Some code, tests, or runtime-free contracts exist, but integration, durability, restart, or end-to-end coverage remains open. |
| Contract target | The document defines the accepted or intended contract and must not be read as proof that the complete runtime path exists. |
| Planned gap | The document explicitly describes future work or a blocked implementation area. |

## Specification register

| Specification | Status | Use this document for |
| --- | --- | --- |
| [AuditLedger v0](AuditLedger_v0.md) | Partial implementation. The contract is accepted for forensic and authorization evidence, but it does not claim database truth or an implemented Admin RPC audit read endpoint. | Durable audit record shape, event families, retention wording, and forensic replay boundaries. |
| [BackupManifest v0](BackupManifest_v0.md) | Implemented evidence, not release approval. Current artifact manifest writer emits explicit v3 bytes and readers preserve v1/v2 compatibility, but restore still requires artifact, WAL archive, PITR, compatibility, and preflight evidence. | Backup manifest identity, artifact wrapper bytes, v3 payload layout, WAL archive validation, retention, immutability, and restore preflight evidence. |
| [BufferPoolPolicy v0](BufferPoolPolicy_v0.md) | Partial implementation. Pinning, dirty tracking, ordered flush candidates, and WAL durability observer evidence exist; checkpoint scheduler, read-ahead scheduler, and commit-visibility owner remain gaps. | Buffer-pool residency, pin lifecycle, dirty LSN tracking, WAL-gated flush, checkpoint candidate ordering, read-ahead boundaries, and visibility separation. |
| [CatalogDiff v0](CatalogDiff_v0.md) | Contract target with partial catalog and contract validation evidence. A dedicated `CatalogDiff` runtime type is not claimed complete. | Catalog object version comparison, diff lifecycle classes, Procedure compatibility integration, dependency impact, plan-cache impact, and diagnostics. |
| [CatalogObjectModel v0](CatalogObjectModel_v0.md) | Contract target with pending catalog crate extraction and future validation gates. | Catalog object identity, versions, dependency graph, contract-hash binding, publication, and recovery evidence. |
| [ContractCompatibility v0](ContractCompatibility_v0.md) | Partial implementation. Current `ExactHash` and `AdditiveOnly` behavior exists, but final compatibility ownership and complete transition coverage remain pending. | Procedure contract transition classes, compatibility inputs, validation order, diagnostics, catalog diff integration, and release gates. |
| [ContractHash Canonicalization v0](ContractHash_Canonicalization_v0.md) | Implemented evidence, not release approval. Core hash behavior has code and golden-test evidence, while standalone public codec and migration policy remain pending. | Stable contract hashing, policy digest behavior, object shape hashes, and canonicalization boundaries. |
| [DatabaseManifest v0](DatabaseManifest_v0.md) | Contract target with current domain-model validation evidence. The durable manifest byte codec is not release-promoted. | Durable recovery-root fields, storage format fingerprints, segment-index roots, manifest switching, recovery behavior, and format-gate validation. |
| [DecisionTrace v0](DecisionTrace_v0.md) | Partial implementation. Compact decision traces, envelopes, correlation, and specialized event variants exist; the complete target trace is not encoded as one concrete Rust struct. | Decision evidence envelope, bounded fields, decision families, audit-safe content, durability and visibility boundaries, retention, and replay. |
| [DefinitionBatch v0](DefinitionBatch_v0.md) | Contract target with current catalog behavior and pending extraction. | DefinitionBatch ordering, dry-run, all-or-nothing apply, WAL publication, recovery, and diagnostics. |
| [DurableCommitEvidence v0](DurableCommitEvidence_v0.md) | Partial implementation. Durable commit LSN guardrails, WAL record evidence, recovery planning, and observability projections exist; no complete runtime packet serializes this structure everywhere. | Durable commit evidence source, required fields, projection fields, validation rules, replay reconstruction, and WAL visibility boundaries. |
| [DurableRollbackEvidence v0](DurableRollbackEvidence_v0.md) | Partial implementation. Durable rollback LSN guardrails, WAL record evidence, recovery planning, and observability projections exist; no complete runtime packet serializes this structure everywhere. | Durable rollback evidence source, required fields, projection fields, validation rules, replay reconstruction, and rollback visibility boundaries. |
| [FrameHeader RPC v0](FrameHeader_RPC_v0.md) | Implemented evidence, not release approval. Runtime-free frame wire evidence exists; QUIC listener/runtime completion is not claimed. | RPC frame header bytes, frame type codes, stream roles, and ResultStream metadata-before-payload ordering. |
| [GpuBatchPolicy v0](GpuBatchPolicy_v0.md) | Contract target and planned accelerator policy. No GPU scheduler, device adapter, or execution engine is claimed complete. | Off-critical-path GPU policy, allowed job classes, CPU fallback, cancellation, validation, and no-GPU-critical-path rules. |
| [HADR Quorum and Fencing v0](HadrQuorumFencing_v0.md) | Partial implementation. Runtime-free quorum, promotion, fencing, membership, and cluster-security contract evidence exists; transport, manifest publication, crash/recovery drills, token propagation, and runbooks still require release evidence. | HA/DR control-plane ownership, node roles, epochs, quorum membership, write admission, fencing, promotion, no-split-brain rules, and evidence gates. |
| [MapDescriptor v0](MapDescriptor_v0.md) | Contract target with partial catalog and WAL classification evidence. A dedicated Map engine crate is not established. | Map identity, grain, summarizability, refresh policy, lineage, publication, staleness, optimizer use, and GPU boundaries. |
| [MapRefreshValidation v0](MapRefreshValidation_v0.md) | Partial implementation. Map and statistics contract evidence exists, but a full Map engine, refresh scheduler, durable Map storage layer, and complete optimizer MapLookup path are not claimed. | Map refresh inputs, staleness gates, summarizability gates, current catalog statistics gates, durable publication, optimizer use, and diagnostics. |
| [MvccIsolation v0](MvccIsolation_v0.md) | Partial implementation. Current MVCC visibility supports `ReadCommitted` and `RepeatableRead`; `Serializable` remains commit metadata without predicate, range, or scheduler enforcement. | MVCC visibility policies, snapshot validation, commit-log isolation metadata, rollback invisibility, long-reader retention, and anomaly labels. |
| [PageHeader and PageTrailer v0](PageHeader_PageTrailer_v0.md) | Implemented evidence, not release approval. Current page codec evidence exists, but this does not promote every heap or B-Tree artifact to release durability. | Explicit page header and trailer layouts, checksum and torn-write evidence, page LSN policy, and format-gate behavior. |
| [PageLifecycle v0](PageLifecycle_v0.md) | Partial implementation. Page identity, layout validation, page-store WAL gating, buffer-pool admission, pin, dirty, flush, and eviction evidence exists; read-ahead, checkpoint, and commit-visibility owners remain gaps. | Page lifecycle ownership, allocation and admission, pin and mutation rules, WAL-gated page flush, checkpoint participation, read-ahead boundaries, and visible commit separation. |
| [PlanCacheKey v0](PlanCacheKey_v0.md) | Partial implementation. Catalog-owned key types, bounded candidate selection, and in-memory cache gates exist, but release evidence still depends on final validation. | Plan-cache identity, invalidation rules, bounded PlanClass taxonomy, explainability, and advisory evidence boundaries. |
| [ProcedureContract v0](ProcedureContract_v0.md) | Implemented evidence, not release approval. Core contract materialization, canonical hashing, binding validation, and compatibility diagnostics exist; full propagation remains an integration gap. | Procedure contract construction, ResultStream rules, binding evidence, compatibility, and validation order. |
| [ProcedureInvocationTrace v0](ProcedureInvocationTrace_v0.md) | Partial implementation. Compact invocation and lifecycle traces exist with test evidence; the complete Procedure invocation trace remains the target contract. | Procedure invocation trace identity, contract binding, lifecycle order, required shape, bounded payload policy, rejection rules, durability, retention, and replay. |
| [RecoveryReport v0](RecoveryReport_v0.md) | Contract target. Recovery and audit vocabulary exists, but the document does not claim every field is encoded in one concrete runtime packet. | Startup modes, replay evidence, corruption and truncation boundaries, forensic constraints, and recovery outcomes. |
| [RecoveryTrace v0](RecoveryTrace_v0.md) | Partial implementation. Compact recovery-related events exist; complete recovery trace evidence is the target contract and is not one concrete encoded Rust struct today. | Recovery trace identity, required trace shape, bounded fields, recovery modes, WAL and corruption boundaries, forensic constraints, RecoveryReport ties, and replay. |
| [SecurityAdmission v0](SecurityAdmission_v0.md) | Partial implementation. Runtime-free admission vocabulary and typed pre-transaction route errors exist; a complete end-to-end admission packet remains future work. | Surface admission, principal and policy evidence, fail-closed behavior, and audit correlation before transaction creation. |
| [SecurityAdmissionCanonicalOrder v0](SecurityAdmissionCanonicalOrder_v0.md) | Contract target over the existing security-admission vocabulary. It does not claim durable audit records for every pre-IAM rejection. | Canonical identity, surface, contract lookup, permission, decision, and audit order. |
| [SegmentIndex v0](SegmentIndex_v0.md) | Contract target with current segment descriptor and contiguity validation evidence. A release-promoted `SegmentIndex v0` disk codec is not exposed. | Durable cold snapshot index bytes, segment entry layout, page and extent range validation, checksums, decode-before-allocate behavior, and recovery handling. |
| [StatsObject v0](StatsObject_v0.md) | Contract target. Existing statistics publication concepts must map to this shape before release claims; a complete runtime statistics engine is not claimed. | StatsVersion lifecycle, histograms, NDV, skew, publication states, advisory optimizer use, and extraction gaps. |
| [StructuredObjectPayload v0](StructuredObjectPayload_v0.md) | Implemented evidence, not release approval. Header metadata, descriptor hashing, row-count policy, and payload bounds are validated; physical payload bytes remain opaque until a separate codec or wire spec defines them. | StructuredObject payload admission, type and absence validation, descriptor hashes, row-count and payload bounds, error classification, and payload-truth boundaries. |
| [SurfaceSeparation v0](SurfaceSeparation_v0.md) | Partial implementation. Security-contract, principal-scope, and ProcedureGateway guards exist; durable IAM, Admin, HA/DR, backup, restore, and forensic runtimes are not claimed. | Application, Administration, HA/DR, monitoring, and forensic surface boundaries, route gates, privileged operation rules, audit correlation, and stream namespace separation. |
| [TypeSystem v0](TypeSystem_v0.md) | Partial implementation. Current type descriptors, absence policy, StructuredObject hashing, and Procedure binding exist; ownership gaps remain explicit. | Scalar families, nullability, decimals, floats, text, semantic identifiers, StructuredObject ties, and compatibility rules. |
| [WalRecord v0](WalRecord_v0.md) | Implemented evidence, not release approval. Current WAL frame and file-header codec evidence exists; multi-segment archive and persisted ChainHash remain pending. | WAL frame bytes, file WAL header bytes, LSN and checksum policy, decode-before-allocate, scan behavior, and recovery validation. |
| [WAL Shipping v0](WalShipping_v0.md) | Partial implementation. Runtime-free WAL shipping contracts and tests exist; QUIC streaming, retry scheduling, persistent multi-segment archives, and end-to-end replica catchup are not release-ready. | WAL shipping source conditions, segment envelopes, replica expectations, chain validation, durable ACKs, backpressure, fencing, promotion, and observability. |

## Related runbooks and ledgers

| Area | Link | Why it matters |
| --- | --- | --- |
| Implementation state | [Implementation Index](../implementation/index.md) | Shows which specs are backed by current code, partial coverage, or open release risks. |
| Operations drills | [Operations Runbook Index](../operations/runbooks/index.md) | Separates implemented durable behavior, contract previews, dry-run behavior, and planned gaps. |
| Architecture decisions | [Architecture Decision Records](../governance/decisions/index.md) | Records decision status, release gates, and cross-references for protocol, security, storage, and recovery. |
| Validation gates | [Step 11 Validation Matrix](../testing/step-11-validation-matrix.md) | Lists release validation areas and crash/recovery, fuzz, Miri, Loom, and supply-chain gaps. |
| Spec-to-test mapping | [Specification Validation Matrix - 2026-05-08](../testing/spec-validation-matrix-2026-05-08.md) | Maps each v0 specification to owner-suite validation areas and residual release gaps. |
| Evidence capture | [Release Evidence Template](../testing/release-evidence-template.md) | Provides the format for recording command evidence and residual risk before approval. |

## Validation

This index is documentation-only. Validating a linked specification requires the
targeted commands named in that specification and the release gates in the
implementation and testing ledgers.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| A v0 specification is treated as implemented runtime behavior. | The reader skipped the status legend or current implementation section. | Reclassify the claim using this index, the specification status section, and the implementation ledgers. |
| A spec points to code evidence but no current gate output is available. | The document records code or tests in the tree, not release-candidate validation. | Record exact commands and results in a release evidence artifact before approval. |
| A runbook step seems stronger than the spec status. | Operational wording drifted ahead of implementation evidence. | Use the runbook status table and reword the claim as contract preview or planned gap. |

## References

- [Andromeda Documentation](../README.md)
- [Architecture Index](../architecture/index.md)
- [Implementation Index](../implementation/index.md)
- [Operations Runbook Index](../operations/runbooks/index.md)
- [Step 11 Validation Matrix](../testing/step-11-validation-matrix.md)
- [Specification Validation Matrix - 2026-05-08](../testing/spec-validation-matrix-2026-05-08.md)
