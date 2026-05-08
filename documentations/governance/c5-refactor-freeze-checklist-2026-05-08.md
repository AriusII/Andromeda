# C5 Refactor Freeze Checklist - 2026-05-08

## Purpose

Provide a C5 refactor freeze checklist for Andromeda movement that can affect
durable truth, visible commit, recovery, security-critical admission, catalog
publication, backup, restore, PITR, or HA/DR.

This checklist is a stop-condition tool. It does not approve a release, certify
release readiness, or replace the C4/C5 control matrix.

## Scope

Use this checklist when a refactor touches or can indirectly affect:

- WAL append, flush, scan, durable-prefix evidence, or transaction terminal
  records;
- page, heap, B-Tree, buffer-pool, disk-manager, manifest, segment, or cold
  snapshot behavior;
- recovery replay, startup mode selection, ForensicStart, SafeStart, FastStart,
  or recovery reports;
- MVCC visibility, isolation, version eligibility, garbage collection, long
  readers, backup pins, or replica lag;
- catalog WAL, DefinitionBatch, CatalogVersion publication, ContractHash,
  publication subscribers, or plan invalidation;
- security admission, identity, authorization, permission scope, audit, or
  pre-transaction rejection;
- RPC or QUIC frame contracts, stream mapping, Procedure route admission,
  ResultStream sequencing, or backpressure;
- backup, restore, PITR, WAL archive, HA/DR membership, quorum, fencing,
  promotion, or WAL shipping.

## Non-goals

- Do not use this checklist to claim release readiness.
- Do not replace owner tests, combined crash/recovery drills, fuzz campaigns,
  Miri, Loom, or retained evidence records.
- Do not approve new runtime behavior, wire formats, persistent formats, SRPL
  semantics, or Procedure contract shapes.
- Do not expand the work scope beyond the refactor under review.
- Do not accept benchmark, RAM, temporary, GPU, or advisory output as durable
  truth.

## Prerequisites

Before opening a C5 refactor freeze review, capture:

- refactor owner and affected subsystem owners;
- branch name and full commit SHA;
- current `git status --short` classification;
- owned write set and explicitly excluded files;
- affected crates, tests, fuzz targets, and documentation artifacts;
- exact validation commands planned for the freeze;
- residual risk owner for every skipped, blocked, or partial gate.

The freeze can only close for the affected scope after the checklist rows are
complete or the owner explicitly removes the affected C5 path from scope.

## Freeze Status Model

| Status | Meaning |
| --- | --- |
| `Open` | Refactor has not entered C5 freeze, or required evidence has not been collected. |
| `Blocked` | One or more no-go conditions are present for the affected C5 path. |
| `Partial` | Some owner evidence exists, but combined crash/recovery, fuzz, security, audit, or cross-subsystem evidence is incomplete. |
| `Deferred` | The affected C5 movement is removed from the current refactor scope and tracked as residual risk. |
| `Closed for scope` | Required evidence exists for the exact affected scope and no listed no-go condition remains. This is not release approval. |

## Freeze Checklist

Use this table as the required checklist for any C5 refactor. Mark each row
`Pass`, `Fail`, `Blocked`, `Partial`, or `Deferred` in the evidence packet.

| Area | Freeze check | Required evidence | Stop condition |
| --- | --- | --- | --- |
| Scope control | The refactor has a written affected-surface list and does not touch unowned files. | Work order, owned write set, `git status --short` classification, and changed-file list. | Unowned C5 files changed without explicit authorization; affected durable paths are unknown; dirty worktree prevents source-state attribution. |
| Doctrine invariants | The design preserves Procedure-only application behavior, durable WAL before visible commit, explicit codecs, GPU exclusion, and surface separation. | Review against `AGENTS.md` and `c4-c5-control-matrix-2026-05-08.md`. | Ad hoc SQL, bypassed Procedure contracts, visible commit without WAL, native Rust struct serialization, GPU in a critical path, or Admin/HA/DR on the Application Surface. |
| WAL | WAL movement preserves append, flush, scan, durable-prefix, checksum, LSN, and transaction terminal evidence. | `cargo test -p andromeda-wal --tests --locked`<br>`cargo test -p andromeda-storage --test wal_scan_recovery_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test wal_durability_fence_contract --locked -- --nocapture`<br>`cargo test -p andromeda-tx --test commit_log_durability --locked -- --nocapture` | Commit visibility can advance before durable WAL; truncated or corrupt records replay as truth; durable-prefix evidence is missing, RAM-derived, or not retained. |
| Storage | Storage movement preserves page, heap, B-Tree, manifest, disk-manager, buffer-pool, and WAL-before-page-flush rules. | `cargo test -p andromeda-storage --tests --locked` plus affected owner tests for page codec, heap vectors, B-Tree vectors, disk durability, and replay contracts. | Dirty page flush outruns durable WAL; incomplete manifest or snapshot publication advances truth; storage format lacks explicit identity, bounds, checksum or digest coverage, or fail-closed rejection. |
| Recovery | Recovery movement preserves deterministic replay, startup classification, and forensic safety. | `cargo test -p andromeda-storage --test crash_recovery_impl --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test recovery_completeness_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test property_recovery_replay --locked -- --nocapture`<br>`cargo test -p andromeda-storage --locked forensic_start -- --nocapture`<br>`cargo test -p andromeda-exec --test recovery_visibility_gates --locked -- --nocapture` | ForensicStart writes durable truth or admits application traffic; recovery accepts unclassified corruption; recovery reports are absent; crash points are not retained in evidence. |
| MVCC | MVCC movement preserves prepared-not-visible, rollback evidence, oldest-snapshot protection, backup pins, replica lag, and retention boundaries. | `cargo test -p andromeda-tx --test v0_transaction_lifecycle --locked -- --nocapture`<br>`cargo test -p andromeda-tx --test mvcc_reclamation_contract --locked -- --nocapture`<br>`cargo test -p andromeda-tx --test mvcc_eligibility_contract --locked -- --nocapture`<br>`cargo test -p andromeda-tx --test mvcc_gc_durability_contract --locked -- --nocapture`<br>`cargo test -p andromeda-tx --test mvcc_isolation_anomaly_contract --locked -- --nocapture` | Prepared state becomes visible before durable commit evidence; GC can reclaim versions still protected by active readers, backup, forensic retention, or replica lag; visibility uses RAM-only state. |
| Catalog publication | Catalog movement preserves all-or-nothing DefinitionBatch publication, catalog WAL replay, ContractHash binding, CatalogVersion switch, subscriber replay, and plan invalidation. | `cargo test -p andromeda-storage --test catalog_wal_contract --locked -- --nocapture`<br>`cargo test -p andromeda-catalog --test wal_record_design --locked -- --nocapture`<br>`cargo test -p andromeda-catalog --test catalog_store_contract --locked -- --nocapture`<br>`cargo test -p andromeda-catalog --test publication_subscription_recovery_contract --locked -- --nocapture`<br>`cargo test -p andromeda-srpl --test definitionbatch_compat --locked -- --nocapture` | Incomplete begin/apply/commit records publish a CatalogVersion; ContractHash mismatch is accepted; Procedure binding can be skipped; stale plans survive incompatible catalog, stats, policy, contract, or shape changes. |
| Security | Security movement fails closed before transaction creation and preserves identity, permission, surface, contract, and audit evidence. | `cargo test -p andromeda-security-contract --lib --locked -- --nocapture`<br>`cargo test -p andromeda-security-contract --test permission_admission_contract --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test iam_pipeline_e2e --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test iam_hardening --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test exec_audit_completion_validation --locked -- --nocapture`<br>`cargo test -p andromeda-observe --test admission_audit_contract --locked -- --nocapture` | Missing identity, missing policy, wrong surface, denied permission, or ContractHash mismatch can create transaction scope; audit is missing for accepted or rejected paths; `SecurityAdmission v0` is overstated as full durable IAM runtime. |
| RPC and QUIC | Protocol movement preserves explicit frame bytes, typed envelope lockstep, stream role separation, metadata-before-payload, ResultStream terminal metadata, and backpressure. | `cargo test -p andromeda-rpc-protocol --tests --locked`<br>`cargo test -p andromeda-quic --test codec_contract --locked -- --nocapture`<br>`cargo test -p andromeda-quic --test protocol_stability_contract --locked -- --nocapture`<br>`cargo test -p andromeda-quic --test protobuf_projection_contract --locked -- --nocapture`<br>`cargo test -p andromeda-quic --test procedure_gateway_route --locked -- --nocapture`<br>`cargo test -p andromeda-quic --test zero_rtt_admission_policy --locked -- --nocapture` | gRPC, runtime JSON default, untyped invocation, ad hoc SQL, wrong stream role, typed envelope drift, missing terminal metadata, or fail-open zero-RTT behavior enters the path. |
| Backup and HA/DR | Backup and cluster movement preserves backup manifest coverage, WAL archive range, PITR target validation, restore staging, audit, quorum, fencing, promotion eligibility, and single-primary behavior. | `cargo test -p andromeda-storage --test backup_physical_plan_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test backup_execution_plan --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test restore_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test hadr_promotion_runtime_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test hadr_membership_store_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test quorum_membership_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test wal_shipping_reclaimability_contract --locked -- --nocapture`<br>`cargo test -p andromeda-quic --test hadr_stream_mapping_contract --locked -- --nocapture` | PITR accepts a target outside WAL coverage; restore publishes partial staged state; promotion lacks quorum, fencing, durable LSN eligibility, or membership proof; split-brain remains possible; backup or HA/DR is exposed through the Application Surface. |
| Fuzz evidence | Affected byte, parser, protocol, ResultStream, admission, contract, DefinitionBatch, page, manifest, segment, or WAL surfaces have sustained fuzz evidence. | Relevant targets from `tests/fuzzing/targets.toml`, including `wal_record_roundtrip`, `page_codec_v1_decode`, `heap_page_v1_decode`, `btree_node_v1_decode`, `manifest_decode`, `segment_index_decode`, `rpc_protocol_frame_codec_decode`, `quic_typed_frame_envelope_decode`, `proto_rpc_execute_request_decode`, `proto_invocation_response_sequence_decode`, `security_contract_admission_matrix`, `contract_hash`, and `definition_batch`. | Compile-only or smoke fuzz output is used as promotion proof; fuzz output is used as a substitute for crash/recovery, authorization, transport, or durable audit evidence. |
| Deep validation | Unsafe, memory-sensitive, or concurrency-sensitive movement has targeted Miri, Loom, or equivalent retained evidence. | Miri evidence for unsafe or memory-sensitive code; Loom evidence for lock manager, ResultStream backpressure, QUIC stream concurrency, WAL append concurrency, buffer-pool pin/flush concurrency, or other interleaving-sensitive paths. | Unsafe drift is undocumented; concurrency behavior is promoted without interleaving evidence; panic, `unwrap`, or `expect` remains in a critical runtime path. |
| Evidence packet | Every pass, fail, skip, manual decision, and residual risk is retained and tied to the exact source state. | Evidence record with command, date, toolchain, platform, commit, branch, artifact path, result, skipped gates, residual risk, and owner review. | Missing artifact, future-dated proof, stale decision, partial owner test, or unreviewed manual note is used as pass evidence. |

## Procedure

1. Open the freeze review when the refactor first touches a C5 path.
2. Record the affected subsystem rows from this checklist.
3. Stop broad movement until each affected row has planned owner evidence.
4. Run the narrowest owner tests first, then the combined crash/recovery,
   security, protocol, backup, HA/DR, fuzz, Miri, or Loom evidence that applies
   to the path.
5. Classify each row as `Pass`, `Fail`, `Blocked`, `Partial`, or `Deferred`.
6. Record every skipped command and every partial result as residual risk.
7. Close the freeze only for the exact reviewed scope after all no-go conditions
   are absent or the affected C5 path is removed from scope.
8. Keep any release approval, release branch decision, or candidate acceptance
   in a separate current-dated artifact.

## Validation

This checklist was prepared as documentation-only governance. It does not show
that any listed command has passed on the current worktree.

To validate a completed freeze review, attach evidence for:

- scope and changed-file attribution;
- doctrine invariant review;
- WAL durable-prefix and visible-commit gates;
- storage and recovery crash-point gates;
- MVCC visibility and garbage-collection gates;
- catalog publication and ContractHash gates;
- security admission, authorization, and audit gates;
- RPC, QUIC, typed envelope, ResultStream, and backpressure gates;
- backup, restore, PITR, quorum, fencing, promotion, and WAL-shipping gates;
- sustained fuzz runs for affected malformed-input surfaces;
- Miri or Loom evidence when unsafe, memory-sensitive, or concurrency-sensitive
  behavior is in scope;
- residual risk and owner sign-off for each deferred item.

## Troubleshooting

If a refactor touches a C5 path accidentally, pause the refactor, record the new
scope, and add the affected checklist rows before continuing.

If a listed owner test has moved, inspect the owning crate before replacing the
command. Do not satisfy the checklist by moving tests between crates.

If a command cannot run because the local environment is missing a toolchain,
linker, or optional tool, mark the row `Blocked` or attach retained CI evidence
from a configured host.

If only isolated owner tests pass, mark the row `Partial` until combined
cross-subsystem evidence exists.

If a no-go condition appears, do not close the freeze for that path. Remove the
path from scope or correct the condition and rerun the affected evidence.

## References

- `AGENTS.md`
- `documentations/governance/c4-c5-control-matrix-2026-05-08.md`
- `documentations/governance/release-readiness-gates-2026-05-08.md`
- `documentations/governance/risk-register-2026-05-08.md`
- `documentations/testing/step-11-validation-matrix.md`
- `documentations/testing/release-evidence-template.md`
- `documentations/WORKER_EXECUTION_MATRIX_2026.md`
- `fuzz/VALIDATION_MATRIX.md`
- `tests/fuzzing/targets.toml`
