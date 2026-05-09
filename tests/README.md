# Test Roadmap Index

## Purpose

This directory is a roadmap index for cross-crate validation. It does not own executable Rust test suites.

Andromeda keeps executable tests in the crates that own the behavior being validated. The historical roadmap labels `tests/integration`, `tests/recovery`, `tests/rpc`, `tests/srpl`, `tests/storage`, `tests/security`, and `tests/fixtures` map to existing crate-owned suites or evidence owners instead of new root-level test directories. Do not recreate README-only directories for these labels.

The standalone Loom model is deliberately isolated under `tools/loom-models/` and is not a root workspace member.

## Scope

Use this index when a work order asks for roadmap test coverage by domain:

| Roadmap label | Crate-owned validation suites |
| --- | --- |
| `tests/integration` | `crates/andromeda-exec/tests/integration_execution_path.rs`, `crates/andromeda-exec/tests/v0_vertical_e2e.rs`, `crates/andromeda-exec/tests/inventory_runtime_e2e_gates.rs`, `crates/andromeda-exec/tests/runtime_contract.rs`, `crates/andromeda-exec/tests/executor_validation_gates.rs`, `crates/andromeda-exec/tests/metadata_extraction_contract.rs`, `crates/andromeda-exec/tests/tx_commit_log_wal_bridge.rs` |
| `tests/recovery` | `crates/andromeda-storage/tests/crash_recovery_impl.rs`, `crates/andromeda-storage/tests/recovery_completeness_contract.rs`, `crates/andromeda-storage/tests/property_recovery_replay.rs`, `crates/andromeda-storage/tests/wal_scan_recovery_contract.rs`, `crates/andromeda-storage/tests/file_wal_recovery_contract.rs`, `crates/andromeda-storage/tests/recovery_replay_heap_redo_contract.rs`, `crates/andromeda-storage/tests/recovery_replay_page_records_contract.rs`, `crates/andromeda-storage/tests/wal_durability_fence_contract.rs`, `crates/andromeda-storage/tests/disk_manager_durability_crash_safety.rs`, `crates/andromeda-exec/tests/recovery_visibility_gates.rs`, `crates/andromeda-transaction/tests/commit_log_durability.rs`, `crates/andromeda-transaction/tests/tx_wal_replay_recovery.rs`, `crates/andromeda-transaction/tests/storage_tx_wal_adapter_contract.rs`, `crates/andromeda-wal/tests/*` |
| `tests/rpc` | `crates/andromeda-rpc-protocol/tests/frame_wire_contract.rs`, `crates/andromeda-rpc-protocol/tests/forbidden_surface_drift.rs`, `crates/andromeda-quic/tests/procedure_gateway_route.rs`, `crates/andromeda-quic/tests/protobuf_projection_contract.rs`, `crates/andromeda-quic/tests/protocol_stability_contract.rs`, `crates/andromeda-quic/tests/codec_contract.rs`, `crates/andromeda-quic/tests/transport_contract.rs`, `crates/andromeda-exec/tests/remote_invoke_network_e2e.rs`, `crates/andromeda-exec/tests/result_stream_backpressure.rs` |
| `tests/srpl` | `crates/andromeda-srpl/tests/validation_gates.rs`, `crates/andromeda-srpl/tests/compiler_pipeline_e2e.rs`, `crates/andromeda-srpl/tests/definitionbatch_compat.rs`, `crates/andromeda-srpl/tests/property_parser_fuzz.rs`, `crates/andromeda-srpl/tests/optimizer_*`, `crates/andromeda-srpl-parser/tests/owner_direct.rs`, `crates/andromeda-srpl-ast/tests/owner_direct.rs` |
| `tests/storage` | `crates/andromeda-storage/tests/storage_hotcold_pipeline_e2e.rs`, `crates/andromeda-storage/tests/page_ownership_invariants.rs`, `crates/andromeda-storage/tests/heap_*`, `crates/andromeda-storage/tests/btree_*`, `crates/andromeda-storage/tests/property_*`, `crates/andromeda-storage/tests/wal_*`, `crates/andromeda-storage/tests/backup_*`, `crates/andromeda-storage/tests/hadr_*`, `crates/andromeda-storage/tests/quorum_*`, `crates/andromeda-wal/tests/*` |
| `tests/security` | `crates/andromeda-security-contract/src/*` unit tests, `crates/andromeda-core/tests/principal_integration.rs`, `crates/andromeda-contract/tests/contract_hash_golden.rs`, `crates/andromeda-exec/tests/iam_pipeline_e2e.rs`, `crates/andromeda-exec/tests/iam_hardening.rs`, `crates/andromeda-exec/tests/permission_scope_contract.rs`, `crates/andromeda-exec/tests/exec_audit_completion_validation.rs`, `crates/andromeda-quic/tests/certificate_continuity_contract.rs`, `crates/andromeda-quic/tests/zero_rtt_admission_policy.rs`, `crates/andromeda-quic/tests/procedure_gateway_route.rs` |
| `tests/fixtures` | Crate-specific fixtures, builders, mocks, and golden vectors stay with the owning crate; fuzz seeds and corpus metadata stay in `fuzz/` and `tests/fuzzing/`; shared deterministic helpers require a dedicated owner and validation command. |

## Root Layer Indices

These roadmap directories are documentation indices. They do not own executable harnesses until a later work order explicitly adds one with crate ownership, validation gates, and release policy.

| Root path | Purpose |
| --- | --- |
| `tests/crash-recovery/` | Index deterministic crash, replay, durability, and visibility scenarios that remain owned by storage, WAL, transaction, catalog, map, or executor crates. |
| `tests/fuzzing/` | Index fuzz and corpus work for untrusted input, persisted bytes, parsers, codecs, canonicalization, and state machines. |
| `tests/miri/` | Index Miri and undefined-behavior checks for unsafe, aliasing, layout, and FFI-sensitive Rust code. |
| `tools/loom-models/` | Standalone Loom model project for bounded concurrency checks that are not yet wired into an owning crate. |
| `benches/` | Placeholder for benchmark governance. Benchmark evidence is advisory and cannot replace correctness, durability, recovery, or security gates. |
| `benches/scenario-evidence/` | Placeholder for ScenarioEvidence benchmark records, workload metadata, and evidence review criteria. |

## Non-goals

- Do not move crate-owned integration tests into this root directory.
- Do not create root-level test harnesses that bypass crate ownership.
- Do not treat fuzz, golden-vector, benchmark, RAM, or temporary output as C5 truth.
- Do not use this index to weaken WAL-before-visible-commit, typed Procedure, RPC, IAM, or recovery invariants.

## Prerequisites

- Rust toolchain from `rust-toolchain.toml`.
- Workspace dependencies resolved with the root `Cargo.lock`.
- `cargo-nextest`, `cargo-audit`, `cargo-deny`, `cargo-fuzz`, and nightly Miri installed only when running the corresponding optional gates.

## Procedure

1. Pick the roadmap label that matches the requested validation area.
2. Run the owning crate tests listed for that label.
3. Add property, fuzz, Miri, Loom, crash/recovery, or release evidence when the change touches C4/C5 behavior.
4. Record gaps in `docs/testing/release-gates.md` rather than moving tests.

## Acceptance Criteria

- Root test directories do not own executable Rust. The isolated Loom project remains under `tools/loom-models/` until an owning crate exposes a stable model adapter.
- Each new test entry names the risk class, owning subsystem, target behavior, and command that validates it.
- Crash/recovery entries state the crash point, durable evidence, replay expectation, and post-recovery visibility assertion.
- Fuzzing entries state the input surface, corpus source, target invariant, and deterministic regression path.
- Loom and Miri entries state the bounded model or undefined-behavior risk and the exact crate command used to reproduce it.
- Benchmark entries state the scenario, metric, hardware profile, confidence limits, disable path, and correctness gate that remains authoritative.
- No root index weakens typed Procedure contracts, WAL-before-visible-commit, IAM, audit, recovery, or RPC boundary requirements.

## Validation

Recommended baseline:

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
cargo test --doc --workspace
cargo audit
cargo deny check
cargo test --manifest-path tools/loom-models/Cargo.toml --locked
```

Recommended C5 smoke gate:

```powershell
cargo test -p andromeda-wal --tests --locked
cargo test -p andromeda-storage --test crash_recovery_impl --locked -- --nocapture
cargo test -p andromeda-storage --test recovery_completeness_contract --locked -- --nocapture
cargo test -p andromeda-storage --test property_recovery_replay --locked -- --nocapture
cargo test -p andromeda-storage --test wal_durability_fence_contract --locked -- --nocapture
cargo test -p andromeda-exec --test recovery_visibility_gates --locked -- --nocapture
cargo test -p andromeda-exec --test integration_execution_path --locked -- --nocapture
cargo test -p andromeda-transaction --test commit_log_durability --locked -- --nocapture
```

Recommended protocol and security gate:

```powershell
cargo test -p andromeda-rpc-protocol --tests --locked
cargo test -p andromeda-quic --test protocol_stability_contract --locked -- --nocapture
cargo test -p andromeda-quic --test protobuf_projection_contract --locked -- --nocapture
cargo test -p andromeda-quic --test procedure_gateway_route --locked -- --nocapture
cargo test -p andromeda-security-contract --lib --locked -- --nocapture
cargo test -p andromeda-exec --test iam_pipeline_e2e --locked -- --nocapture
cargo test -p andromeda-exec --test exec_audit_completion_validation --locked -- --nocapture
```

Recommended SRPL gate:

```powershell
cargo test -p andromeda-srpl --tests --locked
cargo test -p andromeda-srpl-parser --tests --locked
cargo test -p andromeda-srpl-ast --tests --locked
```

Recommended fuzz and Miri preflight:

```powershell
python fuzz/generators/generate_seed_corpus.py --check
cargo check --manifest-path fuzz/Cargo.toml --locked
cargo +nightly miri test --workspace --all-features
```

## Troubleshooting

If a crate-owned suite is renamed, update this index and the Step 11 matrix together. Keep the executable test at the crate boundary that owns the behavior.

If a C5 suite passes in isolation but fails in the combined gate, treat the combined failure as the release blocker. Integration truth must prove the typed Procedure path, WAL durability, recovery replay, visibility, audit, and RPC/security boundaries together.

## References

- `docs/testing/release-gates.md`
- `docs/testing/testing-strategy.md`
- `docs/governance/release-gates.md`
- `fuzz/VALIDATION_MATRIX.md`
- `docs/adr/README.md`
