# V1 Gap Closure Tracker

Date: 2026-05-06

This tracker summarizes V1 gap closure evidence from the current tree after the
worker fleet changes. It is a working coverage and residual risk artifact, not a
production approval record or V1 readiness claim.

Status values:

| Status | Meaning |
| --- | --- |
| Covered by current tests | Named code paths and tests exist in the tree for the gate. |
| Partially covered | Contract tests exist, but one or more runtime, durability, or integration paths remain open. |
| Not validated in this pass | No gate command was run by this documentation pass. |
| Blocker | A known gap must be closed before a release approval claim. |

## Release Gate Coverage Matrix

| Area | Current coverage | Evidence in current tree | Residual release risk | Next gate |
| --- | --- | --- | --- | --- |
| Durability and transaction replay | Partially covered | `crates/andromeda-tx/tests/tx_wal_replay_recovery.rs`, `crates/andromeda-tx/tests/storage_tx_wal_adapter_contract.rs`, `crates/andromeda-exec/tests/tx_commit_log_wal_bridge.rs`, `crates/andromeda-storage/tests/wal_durability_fence_contract.rs` | Terminal transaction status replay is covered by contracts, but release approval still depends on end-to-end WAL, storage, and execution recovery under one workspace gate. | Run the storage, tx, and exec recovery suites together and record the exact command output in a release approval artifact. |
| Storage recovery and page formats | Partially covered | `crates/andromeda-storage/tests/recovery_completeness_contract.rs`, `crates/andromeda-storage/tests/recovery_replay_heap_redo_contract.rs`, `crates/andromeda-storage/tests/recovery_replay_page_records_contract.rs`, `crates/andromeda-storage/tests/disk_page_store_integrity.rs`, `crates/andromeda-storage/tests/btree_node_golden_vectors.rs`, `crates/andromeda-storage/tests/btree_durable_promotion_contract.rs` | Heap and page replay coverage improved, and the B-Tree durable promotion contract now records fail-closed and access-path rebuild evidence. It still must not be treated as full page-backed B-Tree mutation promotion without retained execution and matching implementation proof. | Run `cargo test -p andromeda-storage --test btree_durable_promotion_contract --locked -- --nocapture` with the combined recovery gate and retain the artifact. |
| Execution and Procedure Invocation | Partially covered | `crates/andromeda-exec/tests/invocation_runtime_completion.rs`, `crates/andromeda-exec/tests/exec_audit_completion_validation.rs`, `crates/andromeda-exec/tests/metadata_extraction_contract.rs`, `crates/andromeda-exec/tests/multi_procedure_handlers.rs`, `crates/andromeda-exec/tests/recovery_visibility_gates.rs`, `crates/andromeda-exec/tests/c5_combined_release_gate.rs` | Invocation completion, metadata, and multi-Procedure paths have tests, and the combined C5 release-gate test is present. Release approval still depends on retained execution of that exact command plus the surrounding workspace, supply-chain, fuzz, Miri, and recovery evidence. | Run `cargo test -p andromeda-exec --test c5_combined_release_gate --locked -- --nocapture` and verify no Procedure path creates a transaction after contract rejection. |
| Network and QUIC RPC | Partially covered | `crates/andromeda-quic/tests/real_quinn_network.rs`, `crates/andromeda-quic/tests/reconnect_quinn_admission_contract.rs`, `crates/andromeda-quic/tests/certificate_continuity_contract.rs`, `crates/andromeda-quic/tests/zero_rtt_admission_policy.rs`, `crates/andromeda-quic/tests/zero_rtt_doctrine_contract.rs` | Runtime Quinn coverage exists, but reconnect, pooling, certificate continuity, and retry semantics must be validated with execution-level Invocation behavior. | Run QUIC runtime tests with exec retry tests and keep 0-RTT mutation requests rejected by default. |
| Audit and observability | Partially covered | `crates/andromeda-observe/tests/durable_audit_sink_contract.rs`, `crates/andromeda-observe/tests/durable_audit_query_contract.rs`, `crates/andromeda-observe/tests/durable_audit_retention_contract.rs`, `crates/andromeda-exec/tests/exec_audit_completion_validation.rs`, `crates/andromeda-cli/tests/audit_cli_commands.rs` | Durable audit file sink and read paths are present, but release approval still needs restart evidence tied to security, admin, recovery, and Procedure Invocation families. | Run durable audit sink, read, and retention suites and verify fail-closed behavior is wired into CLI and execution paths. |
| CLI operations | Partially covered | `crates/andromeda-cli/tests/benchmark_cli_commands.rs`, `crates/andromeda-cli/tests/audit_cli_commands.rs`, `crates/andromeda-cli/tests/cli_admin_commands.rs`, `crates/andromeda-cli/src/hadr/runtime.rs`, `crates/andromeda-cli/src/cmd_restore.rs` | CLI surfaces are moving from dry-run and scaffold behavior toward file-backed runtime paths, but some commands still advertise contract previews or require external durable services. | Classify every command as durable backend, contract preview, or dry-run, and fail tests when output overstates durability. |
| Fuzz and parser hardening | Covered by current targets, not validated in this pass | `fuzz/targets.toml`, `fuzz/fuzz_targets/frame_codec_no_panic.rs`, `fuzz/fuzz_targets/proto_frame_envelope_decode.rs`, `fuzz/fuzz_targets/storage_wal_record_roundtrip.rs`, `fuzz/fuzz_targets/btree_node_v1_decode.rs`, `fuzz/fuzz_targets/heap_page_v1_decode.rs`, `fuzz/fuzz_targets/page_codec_v1_decode.rs`, `fuzz/fuzz_targets/segment_index_decode.rs`, `fuzz/fuzz_targets/quic_zero_rtt_admission.rs` | Fuzz target coverage exists for protocol, WAL, page, B-Tree, heap, SegmentIndex, SRPL, and 0-RTT admission, but this pass did not run fuzz smoke or sustained fuzz. | Run the `release-gate-chain.yml` fuzz smoke job or the equivalent local cargo-fuzz sequence before release approval; retain sustained fuzz evidence for promoted surfaces. |
| Map publication | Partially covered | `crates/andromeda-maps/tests/map_publication_contract.rs` | The Map owner suite exists and covers candidate validation, active switch, rollback, rebuild, recovery value evidence, and rebuildable-projection rules. This does not prove runtime catalog/WAL recovery or release execution by itself. | Run `cargo test -p andromeda-maps --test map_publication_contract --locked -- --nocapture` and attach runtime/catalog/WAL evidence if Map publication is in release scope. |
| Miri and Loom evidence | Covered by inventory, not validated in this pass | `tools/testing/miri_subset.py`, `documentations/testing/miri-subset-2026-05-08.md`, `tests/loom/Cargo.toml`, `tests/loom/tests/wal_durable_before_visible.rs`, `.github/workflows/06-nightly-deep-validation.yml` | The Miri subset helper and standalone Loom command are present, so the missing-artifact blockers are obsolete. Miri was not executed as a blocking release gate, and the standalone Loom job remains advisory unless retained and scoped. | Run a blocking or release-reviewed Miri subset and `cargo test --manifest-path tests/loom/Cargo.toml` when concurrency evidence is in scope; add owner-crate Loom models for production interleavings. |
| Release evidence packet | Partially covered | `tools/testing/release_evidence.py`, `tools/testing/release_evidence_schema.md`, `documentations/testing/release-evidence-template.md`, `documentations/testing/ci-release-gate-evidence.md` | The generator captures metadata and declared checks only. It does not run gates, retain artifacts by itself, or approve readiness. | Run `python -B tools/testing/release_evidence.py --json` and attach retained command logs for every pass, fail, skip, gap, and residual risk. |
| Benchmark and performance regression | Partially covered | `crates/andromeda-bench/src/runner.rs`, `crates/andromeda-bench/src/regression_detection.rs`, `crates/andromeda-bench/src/*_benchmark.rs`, `.github/workflows/release-gate-chain.yml` performance-regression job | Benchmark workloads and regression detection exist, but production performance baselines cannot be claimed without the regression gate output for this candidate. | Run the performance-regression job and archive generated evidence before declaring any performance gate closed. |
| Protocol doctrine and schema | Partially covered | `crates/andromeda-proto/tests/protocol_contract.rs`, `crates/andromeda-proto/tests/protobuf_determinism_tests.rs`, `crates/andromeda-proto/tests/contract/schema_governance_contract.rs`, `.github/scripts/protocol_doctrine_scan.py`, `.github/scripts/tests/test_protocol_doctrine_scan.py`, `.github/scripts/andromeda_policy_gate.py` | Protobuf and doctrine scans have tests, but protocol coverage still depends on running the doctrine and policy scans against the final candidate tree. | Run schema governance tests, the protocol doctrine scan, and the policy gate for gRPC, ad hoc SQL examples such as `SELECT *`, runtime JSON drift, and 0-RTT policy. |

## Residual Risk Register

| Risk ID | Risk | Severity | Likelihood | Owner | Status | Mitigation or trigger |
| --- | --- | --- | --- | --- | --- | --- |
| RISK-V1-021 | Release docs overstate readiness by carrying forward PASS or production-ready language from older decision records without rerunning current gates. | High | Medium | Release Governance | Open | Treat this tracker as the current working status. A release approval record must include exact gate commands, dates, and outputs for the candidate tree. |
| RISK-V1-022 | B-Tree mutation and recovery behavior may be inferred from format, golden-vector, or durable promotion contract tests even though page-backed inline mutation promotion remains gated. | High | Medium | Storage Engine | Open | Keep B-Tree durable promotion blocked until retained `btree_durable_promotion_contract` evidence is joined with the combined recovery gate and any promoted implementation proof. |
| RISK-V1-023 | CLI commands may appear durable while still using dry-run, contract preview, or partially wired runtime behavior. | Medium | Medium | CLI Runtime | Open | Require command-level output fields for `durable_backend`, `contract_preview`, and `dry_run`; test each command family. |
| RISK-V1-024 | Durable audit evidence may not yet prove restart readback across all critical event families. | High | Medium | Observability | Open | Run sink, read, retention, and CLI audit suites with restart scenarios for security, admin, recovery, and Procedure Invocation traces. |
| RISK-V1-025 | Network retry and reconnect contracts may pass in isolation but fail to preserve Invocation idempotence, certificate continuity, or result ordering with the execution runtime. | High | Low | QUIC Runtime | Open | Combine QUIC reconnect tests with exec retry and result stream tests before release approval. |
| RISK-V1-026 | Fuzz and property coverage exists but may not have been executed on the final candidate after worker edits. | Medium | Medium | Test Verification | Open | Run fuzz smoke and property tests from `release-gate-chain.yml`; archive target list, seed generation, and results. |
| RISK-V1-027 | Benchmark regressions may be hidden if only workload definitions are inspected. | Medium | Medium | Benchmark Evidence | Open | Run the performance-regression gate and preserve the generated report as release evidence. |
| RISK-V1-028 | Protocol doctrine drift can enter through generated files, helper scripts, or comments if the final scan is skipped. | Critical | Low | Protocol Governance | Open | Run Protobuf schema governance tests and protocol doctrine scan on the release candidate. |
| RISK-V1-029 | Miri subset and Loom standalone inventories may be mistaken for executed release evidence. | High | Medium | Test Verification | Open | Treat `miri_subset.py` dry-run output and continue-on-error Loom artifacts as advisory until retained blocking or release-reviewed command logs exist. |
| RISK-V1-030 | Map publication owner tests may be mistaken for runtime catalog/WAL recovery proof. | High | Medium | Maps and Catalog | Open | Pair `map_publication_contract` output with runtime, catalog, WAL, rebuild, and recovery evidence when Map publication is in release scope. |
| RISK-V1-031 | Local release evidence JSON may be treated as a release pass even though the generator does not run validation gates. | Critical | Medium | Release Governance | Open | Require retained command logs and release-owner disposition for every generated check record. |

## Recommended Next Gates

Run these gates before any release approval claim:

```powershell
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Run these targeted candidate gates when full workspace validation is too broad
for triage:

```powershell
cargo test -p andromeda-storage --test recovery_completeness_contract --locked -- --nocapture
cargo test -p andromeda-storage --test recovery_replay_heap_redo_contract --locked -- --nocapture
cargo test -p andromeda-storage --test recovery_replay_page_records_contract --locked -- --nocapture
cargo test -p andromeda-storage --test btree_durable_promotion_contract --locked -- --nocapture
cargo test -p andromeda-tx --test storage_tx_wal_adapter_contract --locked -- --nocapture
cargo test -p andromeda-exec --test recovery_visibility_gates --locked -- --nocapture
cargo test -p andromeda-exec --test c5_combined_release_gate --locked -- --nocapture
cargo test -p andromeda-maps --test map_publication_contract --locked -- --nocapture
cargo test -p andromeda-quic --test zero_rtt_doctrine_contract --locked -- --nocapture
cargo test -p andromeda-observe --test durable_audit_sink_contract --locked -- --nocapture
cargo test -p andromeda-cli --test audit_cli_commands --locked -- --nocapture
cargo test -p andromeda-cli --test benchmark_cli_commands --locked -- --nocapture
cargo test -p andromeda-proto --test protobuf_determinism_tests --locked -- --nocapture
cargo test --manifest-path tests/loom/Cargo.toml
python -B tools/testing/miri_subset.py --show-exclusions
python -B tools/testing/release_evidence.py --json
python .github/scripts/andromeda_policy_gate.py
```

Run fuzz and performance gates from `.github/workflows/release-gate-chain.yml` before
sign-off. This pass did not run build, test, fuzz, or benchmark commands.
