# C4/C5 Control Matrix - 2026-05-08

## Purpose

Define the control matrix for C4 and C5 movement in Andromeda as of
2026-05-08.

This matrix maps mission-critical subsystems to required validation evidence and
no-go conditions. It is a governance artifact for refactor and promotion
review. It does not approve a release, certify readiness, or replace retained
test evidence.

## Scope

This matrix applies when a change can affect one or more of the following
subsystems:

- WAL durability and durable-prefix evidence.
- Storage pages, heap, B-Tree, manifests, buffer pool, disk manager, and cold
  snapshot publication.
- Recovery, replay, startup mode selection, and forensic classification.
- MVCC visibility, version garbage collection, isolation, and long-reader
  protection.
- Catalog publication, DefinitionBatch, CatalogVersion, ContractHash, and
  publication subscribers.
- Security admission, identity, authorization, permission evaluation, and audit
  evidence.
- RPC, QUIC, ResultStream, typed frame contracts, and stream boundaries.
- Backup, restore, PITR, WAL archive, HA/DR, quorum, fencing, and promotion.

## Non-goals

- Do not claim release readiness from this matrix.
- Do not replace executable tests, crash drills, fuzz campaigns, Miri, Loom, or
  retained evidence records.
- Do not define new persistent formats, network formats, SRPL semantics, or
  Procedure contracts.
- Do not approve ad hoc SQL, dynamic table names, dynamic predicates,
  shape-shifting returns, or implicit null semantics.
- Do not make GPU, RAM, temporary storage, benchmark output, audit output, or
  predictive evidence database truth.
- Do not expose Administration, backup, restore, PITR, or HA/DR capabilities
  through the Application Surface.

## Prerequisites

Before a C4 or C5 movement review can pass, the reviewer must capture:

- branch name and full commit SHA;
- clean or explicitly classified `git status --short` output;
- exact command lines;
- Rust toolchain and platform details;
- pass, fail, skipped, blocked, or partial result for each command;
- retained artifact paths for test logs, crash reports, fuzz output, and manual
  decisions;
- residual risk, owner, and review date.

Use `documentations/testing/release-evidence-template.md` or an equivalent
current-dated evidence record for every command and manual decision. Missing
evidence is a blocker for the affected C4 or C5 movement.

## Risk Class Model

| Risk class | Meaning | Required posture |
| --- | --- | --- |
| `C4` | Boundary-critical behavior that can affect Procedure admission, typed contracts, RPC framing, authorization, audit, observability, or operational control, but does not by itself advance durable database truth. | Fail closed, prove typed boundaries, retain protocol/security/audit evidence, and add fuzz evidence for malformed input surfaces. |
| `C5` | Durable-truth or safety-critical behavior that can affect visible commit, WAL durability, page or manifest truth, recovery, MVCC visibility, catalog publication, backup, restore, PITR, HA/DR, or security-critical admission. | Require crash/recovery evidence, durable-prefix evidence, fail-closed corruption handling, retained artifacts, and cross-subsystem validation where the path crosses ownership boundaries. |

If a change crosses both classes, use the stricter C5 controls.

## Control Matrix

| Subsystem | Movement risk | Required tests and evidence | No-go conditions |
| --- | --- | --- | --- |
| WAL | C5. WAL append, flush, scan, durable-prefix reporting, terminal transaction records, and `DurableCommitEvidence` can affect visible commit and recovery truth. | `cargo test -p andromeda-wal --tests --locked`<br>`cargo test -p andromeda-wal --test file_wal_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test file_wal_recovery_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test wal_scan_recovery_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test wal_durability_fence_contract --locked -- --nocapture`<br>`cargo test -p andromeda-tx --test commit_log_durability --locked -- --nocapture`<br>Sustained `wal_record_roundtrip` fuzz evidence is required before promoting WAL byte-format changes. | Visible commit can occur without durable WAL evidence; corrupt or truncated tails replay as truth; durable-prefix evidence is absent or derived from RAM only; WAL records use native Rust struct layout as the disk contract; fuzz or benchmark output is treated as recovery proof; owner WAL tests are used alone for an end-to-end visibility claim. |
| Storage | C5. Page, heap, B-Tree, buffer-pool, disk-manager, segment, manifest, and cold snapshot movement can affect reconstructed database truth. | `cargo test -p andromeda-storage --tests --locked`<br>`cargo test -p andromeda-storage --test property_page_codec_v1 --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test heap_golden_vectors --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test heap_page_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test btree_node_golden_vectors --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test disk_manager_durability_crash_safety --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test recovery_replay_heap_redo_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test recovery_replay_page_records_contract --locked -- --nocapture`<br>Sustained `page_codec_v1_decode`, `heap_page_v1_decode`, `btree_node_v1_decode`, `manifest_decode`, and applicable segment fuzz evidence is required before promoting affected byte surfaces. | Dirty pages can flush ahead of durable WAL; RAM, temp storage, buffer cache, GPU output, or benchmark output is treated as truth; incomplete manifests advance the root pointer; page, heap, B-Tree, manifest, or segment bytes lack explicit format identity and rejection behavior; B-Tree mutation durability is claimed without insert, delete, split, merge, WAL replay, and crash recovery evidence together. |
| Recovery | C5. Startup, replay, undo, redo, recovery reports, SafeStart, FastStart, and ForensicStart movement can affect what becomes durable truth after a crash. | `cargo test -p andromeda-storage --test crash_recovery_impl --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test recovery_completeness_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test property_recovery_replay --locked -- --nocapture`<br>`cargo test -p andromeda-storage --locked forensic_start -- --nocapture`<br>`cargo test -p andromeda-storage --test recovery_contract --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test recovery_visibility_gates --locked -- --nocapture`<br>`cargo test -p andromeda-tx --test tx_wal_replay_recovery --locked -- --nocapture`<br>Retain crash transcripts that show crash point, durable state, startup decision, replay boundary, rejected records, and recovered visibility. | ForensicStart writes to durable truth or admits application traffic; corruption, LSN-chain breaks, checksum failures, or truncated records are silently accepted; recovery reports are missing or non-deterministic; isolated owner tests are treated as an end-to-end crash/recovery proof; recovery changes lack retained crash-point evidence. |
| MVCC | C5. Visibility, isolation, version garbage collection, eligibility, and long-reader behavior can make prepared or reclaimed state visible incorrectly. | `cargo test -p andromeda-tx --test v0_transaction_lifecycle --locked -- --nocapture`<br>`cargo test -p andromeda-tx --test commit_log_durability --locked -- --nocapture`<br>`cargo test -p andromeda-tx --test tx_wal_replay_recovery --locked -- --nocapture`<br>`cargo test -p andromeda-tx --test mvcc_reclamation_contract --locked -- --nocapture`<br>`cargo test -p andromeda-tx --test mvcc_eligibility_contract --locked -- --nocapture`<br>`cargo test -p andromeda-tx --test mvcc_gc_durability_contract --locked -- --nocapture`<br>`cargo test -p andromeda-tx --test mvcc_isolation_anomaly_contract --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test recovery_visibility_gates --locked -- --nocapture` | Prepared state can become visible before durable commit evidence; rollback evidence is missing or ignored; MVCC garbage collection can reclaim versions still visible to the oldest active snapshot, backup pin, forensic retention, or replica lag; visibility relies on RAM-only state; isolation anomaly evidence is missing for the affected path; GPU or learned output participates in short OLTP visibility decisions. |
| Catalog publication | C5. DefinitionBatch, catalog WAL records, CatalogVersion publication, ContractHash binding, plan invalidation, and subscriber replay can change the meaning of Procedures. | `cargo test -p andromeda-storage --test catalog_wal_contract --locked -- --nocapture`<br>`cargo test -p andromeda-catalog --test wal_record_design --locked -- --nocapture`<br>`cargo test -p andromeda-catalog --test catalog_store_contract --locked -- --nocapture`<br>`cargo test -p andromeda-catalog --test batch_alter_drop_compat --locked -- --nocapture`<br>`cargo test -p andromeda-catalog --test publication_subscription_recovery_contract --locked -- --nocapture`<br>`cargo test -p andromeda-catalog --test plan_invalidation --locked -- --nocapture`<br>`cargo test -p andromeda-srpl --test definitionbatch_compat --locked -- --nocapture`<br>Sustained `definition_batch` and `contract_hash` fuzz evidence is required for affected malformed-input or hash-boundary movement. | Incomplete begin/apply/commit publication sequences advance catalog truth; ContractHash mismatches are accepted; Procedure binding is bypassed; dynamic table names, dynamic predicates, shape-shifting returns, or implicit null semantics enter SRPL core; plan cache entries survive incompatible CatalogVersion, StatsVersion, ContractHash, policy, or shape changes; DefinitionBatch lacks all-or-nothing recovery evidence. |
| Security | C4/C5. Identity, mTLS mapping, admission, authorization, permission scope, policy evaluation, audit, and security-critical protocol boundaries can admit unauthorized work or create untraceable outcomes. | `cargo test -p andromeda-security-contract --lib --locked -- --nocapture`<br>`cargo test -p andromeda-security-contract --test permission_admission_contract --locked -- --nocapture`<br>`cargo test -p andromeda-core --test principal_contract_projection --locked -- --nocapture`<br>`cargo test -p andromeda-contract --test contract_hash_golden --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test iam_pipeline_e2e --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test iam_hardening --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test permission_scope_contract --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test exec_audit_completion_validation --locked -- --nocapture`<br>`cargo test -p andromeda-observe --test admission_audit_contract --locked -- --nocapture`<br>Sustained `security_contract_admission_matrix` fuzz evidence is required before promoting admission-code or malformed-input behavior. | Admission can create transaction scope before identity, contract, surface, and permission checks pass; denied or accepted paths lack durable audit evidence; audit records are treated as database truth; full durable IAM runtime is claimed from `SecurityAdmission v0` contract tests alone; Administration or HA/DR capabilities are exposed on the Application Surface; fail-open behavior exists for missing identity, missing policy, expired credentials, or wrong surface. |
| RPC and QUIC | C4/C5. RPC frame bytes, QUIC stream mapping, typed envelopes, Procedure route admission, ResultStream sequencing, zero-RTT policy, and backpressure can affect contract boundaries and observable execution. | `cargo test -p andromeda-rpc-protocol --tests --locked`<br>`cargo test -p andromeda-quic --test codec_contract --locked -- --nocapture`<br>`cargo test -p andromeda-quic --test protocol_stability_contract --locked -- --nocapture`<br>`cargo test -p andromeda-quic --test protobuf_projection_contract --locked -- --nocapture`<br>`cargo test -p andromeda-quic --test procedure_gateway_route --locked -- --nocapture`<br>`cargo test -p andromeda-quic --test zero_rtt_admission_policy --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test remote_invoke_network_e2e --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test result_stream_backpressure --locked -- --nocapture`<br>Sustained `rpc_protocol_frame_codec_decode`, `quic_typed_frame_envelope_decode`, `proto_rpc_execute_request_decode`, and `proto_invocation_response_sequence_decode` fuzz evidence is required for affected byte or sequence movement. | gRPC, runtime JSON default, ad hoc SQL, or untyped Procedure invocation is introduced; metadata is emitted after payload; typed envelopes and frame context can diverge; wrong surface or wrong stream role reaches execution; zero-RTT admits state-changing work without policy proof; ResultStream backpressure can drop terminal metadata; fuzz evidence is treated as proof of Quinn runtime, TLS identity, authorization, durable audit, or WAL visibility. |
| Backup and HA/DR | C5. Backup, restore, PITR, WAL archive, single-primary HA/DR, quorum, fencing, promotion, WAL shipping, and cluster streams can affect recoverable truth and operator safety. | `cargo test -p andromeda-storage --test backup_physical_plan_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test backup_execution_plan --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test restore_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --locked forensic_start -- --nocapture`<br>`cargo test -p andromeda-storage --test hadr_promotion_runtime_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test hadr_membership_store_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test quorum_membership_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test wal_shipping_reclaimability_contract --locked -- --nocapture`<br>`cargo test -p andromeda-quic --test hadr_stream_mapping_contract --locked -- --nocapture`<br>`cargo test -p andromeda-observe --test hadr_backup_audit_contract --locked -- --nocapture` | PITR targets outside the archived WAL range are accepted; backup manifests omit base checkpoint, required WAL range, digest, retention, or audit evidence; restore can publish partial staged state; promotion lacks quorum, fencing, durable LSN eligibility, or membership proof; split-brain is possible; HA/DR or backup operations are routed through the Application Surface; local unit contracts are used as full failover or restore drill proof. |

## Cross-Cutting Controls

Apply these controls to every C4 or C5 movement:

| Control | Required behavior | No-go condition |
| --- | --- | --- |
| Typed Procedure boundary | Application behavior enters through typed, cataloged Procedures and validated Procedure contracts. | Ad hoc SQL, untyped invocation, dynamic result shapes, or bypassed ContractHash validation. |
| Durable truth | Reconstructible truth is the latest valid cold snapshot or manifest plus durable WAL from that point. | RAM, temp storage, GPU output, benchmark output, audit output, or advisory evidence becomes truth. |
| Explicit codecs | Persisted and network bytes use explicit codecs, format identity, version fields, bounds, checksums or digests, and fail-closed rejection. | Rust native struct layout is serialized directly to disk or network. |
| GPU exclusion | GPU work stays outside commit, WAL, rollback, recovery, MVCC short visibility, catalog publication, Procedure admission, authorization, and security-critical paths. | GPU dependency, runtime, kernel output, or device memory participates in any forbidden path. |
| Observability | Critical decisions are bounded, versioned, explainable, disableable where adaptive, and traceable to retained evidence. | A critical decision is unobservable, unbounded, non-versioned, or accepted only from benchmark or predictive evidence. |
| Evidence retention | Every required command and manual decision has an artifact with command, date, toolchain, commit, branch, pass/fail status, and residual risk. | Evidence is missing, future-dated, partial, skipped without disposition, or not tied to the source state under review. |

## Procedure

1. Identify the affected subsystem and classify the change as C4, C5, or both.
2. Select the row for each affected subsystem.
3. Add the cross-cutting controls to the subsystem requirements.
4. Run the owner tests and integration tests that match the affected paths.
5. Add sustained fuzz evidence for affected byte, parser, protocol, ResultStream,
   security admission, contract-hash, or DefinitionBatch surfaces.
6. Add crash/recovery, Miri, or Loom evidence when the change affects durable
   truth, memory-sensitive code, or concurrency-sensitive behavior.
7. Record every command, result, artifact path, skipped gate, and residual risk.
8. Treat any no-go condition as a blocker until the affected scope is removed or
   the condition is corrected and validated.

## Validation

This document is validation guidance only. It was prepared from current
governance and testing documentation and does not show that the listed commands
passed on a release candidate.

To validate this matrix as a documentation artifact, reviewers should confirm:

- each required subsystem is present;
- each subsystem has required tests and no-go conditions;
- no row treats owner tests, fuzz, benchmarks, RAM, GPU, or audit output as
  durable truth;
- every release or promotion claim is phrased as blocked unless retained
  evidence exists;
- references remain current after test or file renames.

## Troubleshooting

If a listed command has moved, inspect the owning crate's `tests/` directory and
update this matrix only after confirming the replacement owner suite.

If a command passes but retained artifacts are missing, classify the evidence as
partial. Rerun the command with artifact capture before using it for movement
review.

If fuzz targets compile but do not have sustained run evidence, classify fuzz
coverage as preflight only.

If a C4 boundary change creates transaction scope before admission, contract,
surface, and authorization checks pass, reclassify the work as blocked and
resolve the boundary violation before continuing.

If a C5 path lacks crash/recovery evidence for the exact failure point, classify
the movement as blocked for that path.

## References

- `AGENTS.md`
- `documentations/governance/release-readiness-gates-2026-05-08.md`
- `documentations/governance/risk-register-2026-05-08.md`
- `documentations/governance/adr-backlog-2026-05-08.md`
- `documentations/testing/step-11-validation-matrix.md`
- `documentations/testing/release-evidence-template.md`
- `documentations/WORKER_EXECUTION_MATRIX_2026.md`
- `fuzz/VALIDATION_MATRIX.md`
- `fuzz/targets.toml`
