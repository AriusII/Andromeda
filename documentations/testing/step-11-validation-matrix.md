# Step 11 Validation Matrix

## Purpose

Map the Step 11 roadmap test labels to the existing crate-owned Andromeda validation suites without moving tests.

This matrix keeps test ownership aligned with crate ownership. Root labels such as `tests/recovery` and `tests/rpc` are planning labels only; executable validation remains in the owning Rust crates.

## Scope

This document covers:

- `tests/integration`
- `tests/recovery`
- `tests/rpc`
- `tests/srpl`
- `tests/storage`
- `tests/maps`
- `tests/security`
- Recommended commands for owner suites and C5 gates
- Fuzz, Miri, Loom, and release-readiness gaps

## Non-goals

- Do not create or move executable tests.
- Do not alter crate boundaries or public APIs.
- Do not define new persistent, network, RPC, SRPL, or security formats.
- Do not treat fuzz, property, benchmark, RAM, GPU, or temporary output as durable C5 truth.
- Do not mark a C5 path release-ready without crash/recovery, visibility, audit, and protocol/security evidence where applicable.

## Prerequisites

- Use the root workspace manifest at `Cargo.toml`.
- Use the Rust toolchain pinned by `rust-toolchain.toml`.
- Run commands from the repository root.
- Install optional tooling only for the relevant gates: `cargo-nextest`, `cargo-audit`, `cargo-deny`, `cargo-fuzz`, nightly Miri, and Loom.

## Procedure

1. Select the roadmap label for the change.
2. Run the owner-crate command set from the matrix.
3. Add the C5 combined gate when the change can affect WAL durability, visible commit, recovery, page or manifest truth, Procedure dispatch, RPC framing, catalog publication, IAM, audit, HA/DR, backup, or restore.
4. Record any missing evidence as a release gap. Do not satisfy a missing gate by moving tests to `tests/`.

## Validation Matrix

| Roadmap label | Risk class | Existing owner suites | Recommended commands | Current gaps |
| --- | --- | --- | --- | --- |
| `tests/integration` | C4/C5 when it crosses Procedure, WAL, recovery, RPC, or IAM boundaries | `andromeda-exec` owns execution path evidence through `integration_execution_path.rs`, `v0_vertical_e2e.rs`, `inventory_runtime_e2e_gates.rs`, `runtime_contract.rs`, `executor_validation_gates.rs`, `metadata_extraction_contract.rs`, `tx_commit_log_wal_bridge.rs`, `c4_admission_events.rs`, `c5_commit_rollback_lifecycle.rs`, and `c5_combined_release_gate.rs`. Storage and transaction integration evidence remains in `andromeda-storage` and `andromeda-tx` owner suites. | `cargo test -p andromeda-exec --test integration_execution_path --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test v0_vertical_e2e --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test inventory_runtime_e2e_gates --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test runtime_contract --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test tx_commit_log_wal_bridge --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test c5_combined_release_gate --locked -- --nocapture` | Needs a retained release-recorded `c5_combined_release_gate` run and the surrounding workspace gates before claiming typed Procedure admission, ContractHash rejection, WAL append, durable completion, ResultStream metadata, audit, and recovery visibility are release-ready together. |
| `tests/recovery` | C5 | `andromeda-storage` owns recovery reports, replay planning, manifest/page integration, WAL scan consumption, FileWal recovery integration, B-Tree durable promotion contract evidence, and crash/recovery scenarios. `andromeda-wal` owns pure WAL and physical FileWal owner evidence. `andromeda-tx` owns transaction state and WAL adapter durability evidence. `andromeda-exec` owns application-visible recovery visibility evidence. | `cargo test -p andromeda-wal --tests --locked`<br>`cargo test -p andromeda-storage --test crash_recovery_impl --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test recovery_completeness_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test property_recovery_replay --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test wal_scan_recovery_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test file_wal_recovery_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test recovery_replay_heap_redo_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test recovery_replay_page_records_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test wal_durability_fence_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test btree_durable_promotion_contract --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test recovery_visibility_gates --locked -- --nocapture`<br>`cargo test -p andromeda-tx --test commit_log_durability --locked -- --nocapture`<br>`cargo test -p andromeda-tx --test tx_wal_replay_recovery --locked -- --nocapture` | B-Tree durable promotion has an owner contract path, but release approval still needs retained execution of that command and the combined recovery gate. Vertical ProductStock heap reconstruction after crash still needs a release-recorded end-to-end drill if the release target includes durable ProductStock state. |
| `tests/rpc` | C4/C5 for wire contracts, Procedure dispatch, admission, ResultStream metadata, and HA/DR stream behavior | `andromeda-rpc-protocol` owns explicit frame bytes and forbidden surface drift. `andromeda-quic` owns QUIC mapping, typed envelope projection, route admission, transport contracts, zero-RTT policy, certificate continuity, reconnect behavior, and protocol stability. `andromeda-exec` owns remote invocation and ResultStream behavior above the transport. | `cargo test -p andromeda-rpc-protocol --tests --locked`<br>`cargo test -p andromeda-quic --test codec_contract --locked -- --nocapture`<br>`cargo test -p andromeda-quic --test protocol_stability_contract --locked -- --nocapture`<br>`cargo test -p andromeda-quic --test protobuf_projection_contract --locked -- --nocapture`<br>`cargo test -p andromeda-quic --test procedure_gateway_route --locked -- --nocapture`<br>`cargo test -p andromeda-quic --test zero_rtt_admission_policy --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test remote_invoke_network_e2e --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test result_stream_backpressure --locked -- --nocapture` | Fuzz smoke exists for frame and typed-envelope malformed input, but release promotion still needs sustained fuzz run evidence and a recorded runtime route/admission/audit run. Fuzz does not prove Quinn runtime behavior, TLS identity, authorization, or durable audit. |
| `tests/srpl` | C3/C4 normally; C5 when Procedure contracts, DefinitionBatch publication, catalog binding, or recovery semantics are affected | `andromeda-srpl` owns compiler pipeline, validation gates, DefinitionBatch compatibility, optimizer safety, parser property coverage, and facade compatibility. `andromeda-srpl-parser` and `andromeda-srpl-ast` own their direct owner tests. | `cargo test -p andromeda-srpl --tests --locked`<br>`cargo test -p andromeda-srpl-parser --tests --locked`<br>`cargo test -p andromeda-srpl-ast --tests --locked`<br>`cargo check --manifest-path fuzz/Cargo.toml --bin srpl_parser_signature_decode --locked`<br>`cargo check --manifest-path fuzz/Cargo.toml --bin srpl_parser_owner_decode --locked` | Parser fuzz compile checks exist, but sustained fuzz evidence is still a release gap. DefinitionBatch crash tests for begin, apply, and commit boundaries remain required before treating catalog publication paths as C5-complete. |
| `tests/storage` | C5 for page, heap, WAL, manifest, buffer-pool, backup, restore, HA/DR, and recovery truth | `andromeda-storage` owns page codec integration, heap layout, B-Tree format and deferred mutation gates, durable promotion contract evidence, storage hot/cold pipeline, buffer-pool WAL fences, disk durability, backup, restore, WAL GC, HADR membership, quorum, and publication facade evidence. `andromeda-wal` owns pure WAL and physical FileWal owner contracts. | `cargo test -p andromeda-storage --tests --locked`<br>`cargo test -p andromeda-storage --test property_page_codec_v1 --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test heap_golden_vectors --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test heap_page_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test btree_node_golden_vectors --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test btree_durable_promotion_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test disk_manager_durability_crash_safety --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test backup_physical_plan_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test restore_contract --locked -- --nocapture`<br>`cargo test -p andromeda-wal --tests --locked` | Page, heap, B-Tree, WAL, SegmentIndex, and security fuzz targets compile or have compile commands, but sustained fuzz evidence is still required for promotion. B-Tree durable mutation recovery remains a release gap until retained execution proves the contract with combined recovery evidence. Backup/PITR and HA/DR release approval need combined recovery and fencing evidence, not just unit contracts. |
| `tests/maps` | C4/C5 when analytical Map publication, active switch, rollback, rebuild, or recovery evidence is in scope | `andromeda-maps` owns the Map descriptor and publication value contracts through `map_publication_contract.rs`. Catalog statistics publication remains separate owner evidence in `andromeda-catalog`. | `cargo test -p andromeda-maps --test map_publication_contract --locked -- --nocapture`<br>`cargo test -p andromeda-catalog --test stats_publication_switch_tests --locked -- --nocapture` | The Map owner suite is present, so the missing-suite blocker is obsolete. Release approval still needs retained execution and any runtime/catalog/WAL recovery evidence required by the release scope. |
| `tests/security` | C4/C5 for identity, admission, authorization, audit, and security-critical protocol boundaries | `andromeda-security-contract` owns runtime-free admission and permission-family contracts through library tests. `andromeda-core` owns principal integration. `andromeda-contract` owns ContractHash golden evidence. `andromeda-exec` owns IAM pipeline, hardening, permission scope, audit completion, and pre-transaction rejection evidence. `andromeda-quic` owns certificate continuity, zero-RTT admission, and route security boundaries. | `cargo test -p andromeda-security-contract --lib --locked -- --nocapture`<br>`cargo test -p andromeda-core --test principal_integration --locked -- --nocapture`<br>`cargo test -p andromeda-contract --test contract_hash_golden --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test iam_pipeline_e2e --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test iam_hardening --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test permission_scope_contract --locked -- --nocapture`<br>`cargo test -p andromeda-exec --test exec_audit_completion_validation --locked -- --nocapture`<br>`cargo test -p andromeda-quic --test certificate_continuity_contract --locked -- --nocapture`<br>`cargo test -p andromeda-quic --test zero_rtt_admission_policy --locked -- --nocapture` | Security fuzz and contract tests do not prove durable audit or live policy-store correctness by themselves. Release evidence must show fail-closed admission before transaction creation and durable audit coverage for accepted and rejected paths. |

## C5 Validation Notes

Use the following combined gate before claiming C5 readiness for durable Procedure execution:

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
cargo test --doc --workspace
cargo audit
cargo deny check
cargo test -p andromeda-exec --test c5_combined_release_gate --locked -- --nocapture
cargo test -p andromeda-wal --tests --locked
cargo test -p andromeda-storage --test crash_recovery_impl --locked -- --nocapture
cargo test -p andromeda-storage --test recovery_completeness_contract --locked -- --nocapture
cargo test -p andromeda-storage --test property_recovery_replay --locked -- --nocapture
cargo test -p andromeda-storage --test wal_scan_recovery_contract --locked -- --nocapture
cargo test -p andromeda-storage --test wal_durability_fence_contract --locked -- --nocapture
cargo test -p andromeda-storage --test btree_durable_promotion_contract --locked -- --nocapture
cargo test -p andromeda-exec --test recovery_visibility_gates --locked -- --nocapture
cargo test -p andromeda-exec --test integration_execution_path --locked -- --nocapture
cargo test -p andromeda-exec --test c5_commit_rollback_lifecycle --locked -- --nocapture
cargo test -p andromeda-tx --test commit_log_durability --locked -- --nocapture
```

When Map publication is in scope, add:

```powershell
cargo test -p andromeda-maps --test map_publication_contract --locked -- --nocapture
```

Use the following protocol and security gate before claiming C4/C5 readiness for RPC invocation:

```powershell
cargo test -p andromeda-rpc-protocol --tests --locked
cargo test -p andromeda-quic --test protocol_stability_contract --locked -- --nocapture
cargo test -p andromeda-quic --test protobuf_projection_contract --locked -- --nocapture
cargo test -p andromeda-quic --test procedure_gateway_route --locked -- --nocapture
cargo test -p andromeda-security-contract --lib --locked -- --nocapture
cargo test -p andromeda-exec --test iam_pipeline_e2e --locked -- --nocapture
cargo test -p andromeda-exec --test exec_audit_completion_validation --locked -- --nocapture
```

These gates are necessary but not automatically sufficient. Release approval still needs a recorded run artifact, explicit failure-mode coverage, and a decision that no blocker remains for visible commit, accepted contract mismatch, recovery failure, audit omission, panic in critical paths, silent corruption, non-reproducible crash tests, or unobservable critical decisions.

## Crash/Recovery Scenario Matrix

Use this matrix to convert Step 11 release claims into deterministic crash/recovery evidence. Each scenario requires a release evidence record that includes the exact command, date, toolchain, commit and branch, pass/fail result, artifact path, and residual risk.

| Scenario | Scope | Crash or failure point | Durable state to prove | Required validation evidence | Residual risk to record |
| --- | --- | --- | --- | --- | --- |
| CR-11-WAL | WAL append, flush, scan, and transaction terminal records | Before append, after append before durable flush, during partial record write, after flush before visibility publication | Only checksum-valid flushed WAL prefix is replayable. Truncated or corrupt tails are excluded. A transaction terminal record cannot make commit visible unless its durable LSN is proven. | `cargo test -p andromeda-wal --tests --locked`<br>`cargo test -p andromeda-storage --test file_wal_recovery_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test wal_scan_recovery_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test wal_durability_fence_contract --locked -- --nocapture`<br>`cargo test -p andromeda-tx --test commit_log_durability --locked -- --nocapture` | Owner tests must be joined with storage, tx, and exec visibility evidence before claiming end-to-end durable commit readiness. |
| CR-11-STORAGE | Page, heap, index, buffer-pool, and disk-manager recovery | Before page flush, after page flush before manifest switch, during heap redo, during B-Tree promotion or mutation, and while dirty RAM exists | RAM and temporary state are not truth. Durable pages are reconstructed from the latest valid snapshot/manifest plus durable WAL replay. Page flushes remain behind WAL durability fences. | `cargo test -p andromeda-storage --test disk_manager_durability_crash_safety --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test recovery_replay_heap_redo_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test recovery_replay_page_records_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test property_recovery_replay --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test wal_btree_promotion_gate_recovery --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test btree_durable_promotion_contract --locked -- --nocapture` | The B-Tree durable promotion contract exists and should be retained as release evidence. It still proves rebuild/fail-closed behavior rather than full page-backed inline mutation promotion unless the release packet includes matching implementation and recovery proof. |
| CR-11-MANIFEST | Manifest publication, cold snapshot publication, and root switching | Before manifest write, during snapshot publication, between publication and root switch, and after root switch before cleanup | The latest valid manifest root plus durable WAL defines truth. An incomplete manifest or publication attempt is ignored or classified without advancing durable visibility. | `cargo test -p andromeda-storage --test recovery_completeness_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test wal_durability_fence_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test publication_facade_invariants --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test layout_publication_contract --locked -- --nocapture` | Release approval needs a recorded drill that combines manifest switch, WAL replay, and restore visibility, not isolated publication contracts only. |
| CR-11-CATALOG-PUBLICATION | DefinitionBatch, CatalogVersion, catalog WAL bridge, plan invalidation, and publication subscribers | After catalog publication begin, after apply before commit, after commit before cache invalidation, and during subscriber replay | Only a complete begin/apply/commit sequence advances catalog truth. Incomplete tail records remain forensic evidence and do not publish a new CatalogVersion. | `cargo test -p andromeda-storage --test catalog_wal_contract --locked -- --nocapture`<br>`cargo test -p andromeda-catalog --test catalog_store_contract --locked -- --nocapture`<br>`cargo test -p andromeda-catalog --test batch_alter_drop_compat --locked -- --nocapture`<br>`cargo test -p andromeda-catalog --test publication_subscription_recovery_contract --locked -- --nocapture`<br>`cargo test -p andromeda-srpl --test definitionbatch_compat --locked -- --nocapture` | DefinitionBatch crash tests for begin, apply, and commit boundaries remain required before catalog publication is C5-complete. |
| CR-11-MAP-PUBLICATION | Analytical Map refresh and active publication | After candidate refresh build, before active switch, after active switch before artifact retention, and during refresh cancellation | Source tables, catalog versions, and durable WAL remain canonical truth. Map output is rebuildable and cannot become commit, WAL, recovery, catalog, or security truth. GPU or batch output is never accepted as durable evidence. | Owner evidence: `cargo test -p andromeda-maps --test map_publication_contract --locked -- --nocapture` covers candidate validation, active switch, rollback, rebuild, recovery value evidence, and rebuildable-projection rules. Keep analogous statistics publication evidence separate, for example `cargo test -p andromeda-catalog --test stats_publication_switch_tests --locked -- --nocapture`. | The missing Map owner-suite blocker is obsolete when `map_publication_contract.rs` is present. Release approval still needs retained execution and any runtime/catalog/WAL recovery evidence required by the release scope. |
| CR-11-FORENSIC-STARTUP | FastStart, SafeStart, ForensicStart, recovery reports, and anomaly classification | Clean WAL, recoverable tail truncation, LSN chain break, checksum failure, missing forensic report, and attempted application traffic during forensic mode | ForensicStart is read-only, never replays into durable truth, preserves the forensic report, classifies anomalies, and blocks application traffic. SafeStart and FastStart cannot silently accept evidence requiring forensic handling. | `cargo test -p andromeda-storage --locked forensic_start -- --nocapture`<br>`cargo test -p andromeda-storage --test recovery_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test file_wal_recovery_contract --locked -- --nocapture` | A release drill must attach retained forensic artifacts and show the chosen startup mode, rejected alternatives, and application-surface denial. |
| CR-11-BACKUP-PITR | Backup manifest, physical backup plan, WAL archive, restore orchestration, PITR target validation, and audit | During backup manifest capture, while archiving WAL, before restore preflight, after partial restore staging, and at target LSN boundaries | The backup manifest identifies the base checkpoint and required WAL range. PITR restores only within the archived range and rejects targets before the required start or after the archive end. | `cargo test -p andromeda-storage --test backup_physical_plan_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test backup_execution_plan --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test restore_contract --locked -- --nocapture` | Full backup/restore drills and exact LSN restore evidence remain required before production backup/PITR readiness. |
| CR-11-HADR | Single-primary HA/DR, membership, quorum, fencing, WAL shipping, promotion, and HADR stream mapping | Primary crash after WAL flush, replica lag, network partition, split-brain attempt, lost quorum, promotion race, and stale membership publication | At most one primary can publish. Promotion requires quorum, fencing, and durable LSN eligibility. HA/DR operations stay off the Application Surface. | `cargo test -p andromeda-storage --test hadr_promotion_runtime_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test hadr_membership_store_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test quorum_membership_contract --locked -- --nocapture`<br>`cargo test -p andromeda-storage --test wal_shipping_reclaimability_contract --locked -- --nocapture`<br>`cargo test -p andromeda-quic --test hadr_stream_mapping_contract --locked -- --nocapture` | Local cluster simulation, fencing proof under partition, and failover-with-recovery transcript remain release blockers for production HA/DR claims. |

## Fuzz, Miri, Loom, And Release Gaps

| Gate | Current evidence | Gap to close |
| --- | --- | --- |
| Fuzz | `tests/fuzzing/targets.toml`, `fuzz/VALIDATION_MATRIX.md`, `.github/workflows/07-fuzzing.yml`, and `fuzz/fuzz_targets/segment_index_decode.rs` cover smoke fuzzing for WAL, storage byte formats, SegmentIndex, SRPL parser decode, RPC frame decode, typed QUIC envelope decode, ResultStream sequence validation, and security admission matrices. SegmentIndex compile check: `cargo check --manifest-path fuzz/Cargo.toml --bin segment_index_decode --locked`. | Sustained fuzz run evidence is still required before promotion of byte, parser, protocol, or admission surfaces. Compile checks and 15-second CI smoke runs are not release proof. |
| Miri | `.github/workflows/06-nightly-deep-validation.yml` runs `cargo +nightly miri test --workspace --all-features` as a continue-on-error smoke job and records the `tools/testing/miri_subset.py` inventory when present. | C5 release should record a passing Miri subset for unsafe or memory-sensitive crates if the full workspace job is impractical. Continue-on-error CI and dry-run inventory output do not count as a blocking release gate. |
| Loom | `tests/loom/Cargo.toml` provides the standalone command `cargo test --manifest-path tests/loom/Cargo.toml` for the current WAL durable-before-visible model. `.github/workflows/06-nightly-deep-validation.yml` runs it as continue-on-error advisory evidence. | The root Loom command and model path are present, so the missing-Loom blocker is obsolete. Owner-crate Loom models or explicit scope exclusions are still required before promoting production concurrency claims such as lock manager, ResultStream backpressure, QUIC stream concurrency, WAL append concurrency, or buffer-pool pin/flush concurrency. |
| Crash/recovery | `crates/andromeda-storage/tests/crash_recovery_impl.rs`, `property_recovery_replay.rs`, `recovery_completeness_contract.rs`, `wal_scan_recovery_contract.rs`, `file_wal_recovery_contract.rs`, and `crates/andromeda-exec/tests/recovery_visibility_gates.rs` provide strong existing evidence. `.github/workflows/15-crash-recovery-placeholder.yml` runs a small replay gate. | Expand the release-recorded gate to include storage, tx, exec, WAL owner, and vertical ProductStock replay evidence together. Do not treat isolated owner tests as end-to-end durable visibility proof. |
| Release | Governance docs identify C5 blockers and release approval criteria. `tools/testing/release_evidence.py` can capture local metadata and declared check records. | Step 11 should emit a retained release artifact with exact commands, commit SHA, toolchain, pass/fail status, skipped tests, artifacts, and unresolved gaps. The generator does not run gates or approve readiness. |

## Troubleshooting

If a test listed here cannot be found, inspect the owning crate's `tests/` directory first. Rename the matrix entry only after confirming the new owner path.

If `cargo test -p andromeda-security-contract --lib` is the only available security-contract command, keep it as the owner gate until an external `tests/` suite is added in that crate.

If a root roadmap label conflicts with crate ownership, crate ownership wins. Add cross-crate evidence by composing commands, not by moving tests.

## References

- `tests/README.md`
- `tests/AGENTS.md`
- `docs/codex/rust-critical-quality-gates.md`
- `docs/codex/mission-critical-change-policy.md`
- `fuzz/VALIDATION_MATRIX.md`
- `tests/fuzzing/targets.toml`
- `.github/workflows/06-nightly-deep-validation.yml`
- `.github/workflows/07-fuzzing.yml`
- `.github/workflows/15-crash-recovery-placeholder.yml`
- `tests/loom/Cargo.toml`
- `tools/testing/miri_subset.py`
- `tools/testing/release_evidence.py`
- `documentations/governance/decisions/DEC-035-release-gate-chain.md`
- `documentations/governance/decisions/DEC-036-release-readiness-approval.md`
