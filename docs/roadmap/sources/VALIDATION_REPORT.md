# Roadmap Validation Report

## P03 Result

P03 catalog, `ContractHash`, and durable `DefinitionBatch` mutation are closed under the local strict P03 checker and focused Rust gates. Release readiness remains blocked by non-P03 release evidence and CI workflow gaps.

| Gate | Result | Notes |
|---|---:|---|
| `python -B tools/testing/p03_catalog_definition_batch_check.py --strict` | PASS | Required P03 tests and four retained P03 evidence reports are present; catalog durability, recovery, ContractHash, PlanCache, and publication invariants are found. |
| `python -m py_compile tools/testing/p03_catalog_definition_batch_check.py` | PASS | P03 checker compiles. |
| `cargo test -p andromeda-catalog --test catalog_store_contract --locked -- --nocapture` | PASS | 33 tests pass, covering no half-catalog visibility, durable Begin/Apply/Commit evidence, apply index density, flush failure, and recovery tails. |
| `cargo test -p andromeda-definition-batch --test alter_drop_compat --locked -- --nocapture` | PASS | 11 tests pass, covering additive, breaking, deprecated, rejected, stale base, duplicate apply, and recreate rejection behavior. |
| `cargo test -p andromeda-catalog --test catalog_digest_contract --locked -- --nocapture` | PASS | 8 tests pass, covering catalog digest, dependency graph, and CatalogVersion evidence separate from ContractHash. |
| `cargo test -p andromeda-procedure-contract --test procedure_contract_digest --locked -- --nocapture` | PASS | 13 tests pass, covering canonical ContractHash, SRPL formatting independence, observable shape drift, and binding version evidence. |
| `cargo test -p andromeda-catalog-recovery --test mutation_replay_anomalies --locked -- --nocapture` | PASS | 15 tests pass, covering incomplete, duplicate, sparse, out-of-order, stale/version-gap, and hash-tampered replay. |
| `cargo test -p andromeda-catalog-recovery --test catalog_publication_subscription_runtime_contract --locked -- --nocapture` | PASS | 10 tests pass, covering Administration/HA-only publication reports, durable LSN/marker evidence, replay/invalidation mismatch, duplicate publication rejection, and HADR replay evidence. |
| `cargo test -p andromeda-plan-cache --test plan_invalidation --locked -- --nocapture` | PASS | 10 tests pass, covering CatalogVersion, ContractHash, StatsVersion, PolicyVersion, and advisory evidence trace behavior. |
| `cargo test -p andromeda-catalog -p andromeda-definition-batch -p andromeda-catalog-recovery -p andromeda-procedure-contract -p andromeda-plan-cache --locked` | PASS | Full touched-crate test suites pass, including DefinitionBatch Alter/Drop spec coverage invariants. |
| `cargo check --workspace --locked` | PASS | Workspace type-checks after the P03 runtime publication duplicate-rejection change. |
| `cargo clippy -p andromeda-catalog -p andromeda-definition-batch -p andromeda-catalog-recovery -p andromeda-procedure-contract -p andromeda-plan-cache --tests --locked -- -D warnings` | PASS | Touched crates are clippy-clean for tests and library targets. |
| Targeted `cargo fmt -p ... -- --check` on touched crates | PASS | Rustfmt exits successfully for touched crates while emitting existing config warnings for nightly-only or unknown options. |
| `cargo fmt --all -- --check` | BLOCKED | Windows path length failure: `os error 206`. |
| `python -B tools/testing/p02_durable_vertical_path_check.py --strict` | PASS | P02 remains closed after P03. |
| `python -B tools/testing/p01_spec_baseline_check.py --strict` | PASS | P01 remains closed after P03. |
| `python -B tools/testing/p00_repository_state_check.py --strict` | PASS | P00 remains closed after P03. |
| `git diff --check` | PASS | No whitespace errors in the P03 diff. |

## P03 Decision

P03 closure is complete for the local catalog durability scope. The accepted evidence proves:

- catalog mutation publication uses ordered `CatalogMutationRecord` Begin/Apply/Commit evidence and does not expose a half-catalog on append or flush failure;
- recovery reconstructs only the last valid `CatalogVersion` and reports incomplete/anomalous batches through `CatalogRecoveryReport`;
- canonical `ContractHash` is independent from SRPL source formatting and changes only when observable procedure contract shape changes;
- plan cache identity is tied to `CatalogVersion`, `ContractHash`, `StatsVersion`, `PolicyVersion`, and `PlanClass`;
- publication reports require Administration/HA audience plus durable LSN or marker evidence, and duplicate visible publication evidence is rejected before registry mutation.

The release gate remains closed. Do not claim production readiness until retained release evidence exists for fuzzing, Miri, backup/PITR restore drills, HA/DR cluster drills, B-Tree durable promotion crash/recovery, CI workflows, and final release evidence packets.

## P02 Result

P02 durable `Inventory.ReserveStock` / `Inventory.ProductStock` vertical path is closed under the local strict P02 checker and focused Cargo gates. Release readiness remains blocked by non-P02 evidence and CI workflow gaps.

| Gate | Result | Notes |
|---|---:|---|
| `python -B tools/testing/p02_durable_vertical_path_check.py --strict` | PASS | Required P02 tests and the four retained P02 evidence reports are present; durable vertical invariants are found. |
| `python -m py_compile tools/testing/p02_durable_vertical_path_check.py` | PASS | P02 checker compiles. |
| `cargo test -p andromeda-inventory-demo --test v0_vertical_e2e --locked -- --nocapture` | PASS | 25 tests pass, covering ProductStock publication, pre-transaction rejection, crash-before-client-ACK recovery, and ResultStream ordering. |
| `cargo test -p andromeda-wal --test file_wal_contract --locked -- --nocapture` | PASS | FileWal owner contract tests pass. |
| `cargo test -p andromeda-recovery --test file_wal_recovery_contract --locked -- --nocapture` | PASS | FileWal recovery report and replay contract tests pass. |
| `cargo test -p andromeda-storage-heap --test product_stock_heap_contract --locked -- --nocapture` | PASS | ProductStock heap/HREDOV1 contract tests pass. |

## P02 Decision

P02 closure is complete for the local durable vertical path. The accepted evidence proves:

- invalid protocol, contract, admission, and permission inputs reject before WAL append or ProductStock mutation;
- ProductStock publication requires `product_stock_commit`, `durable_commit_lsn`, and HREDOV1 `redo_record_lsn` evidence;
- crash before client ACK recovers ProductStock rows from durable FileWal records, not from ResultStream or client-visible output;
- ResultStream emits metadata, batch, and terminal completion, with completion carrying non-zero durable LSN evidence.

The release gate remains closed. Do not claim production readiness until retained release evidence exists for fuzzing, Miri, backup/PITR restore drills, HA/DR cluster drills, B-Tree durable promotion crash/recovery, CI workflows, and final release evidence packets.

## P01 Result

P01 normative specification baseline is closed under the hardened local checker. Release readiness remains blocked by non-P01 evidence and CI workflow gaps.

| Gate | Result | Notes |
|---|---:|---|
| `python -B tools/testing/p01_spec_baseline_check.py --strict` | PASS | All required P01 specs are present, indexed, sectioned, grounded, owned, and carry concrete `Owner` / `Evidence` / `Reject` acceptance summaries. |
| `python -B tools/testing/p00_repository_state_check.py --strict` | PASS | P00 repository-state baseline remains coherent after P01 specification hardening. |
| `python -m py_compile tools/testing/p01_spec_baseline_check.py tools/testing/roadmap_gate_summary.py tools/testing/validation_manifest.py` | PASS | Validation scripts compile. |
| `git diff --check` | PASS | No whitespace errors in the P01 diff. |
| `cargo check --workspace --locked` | PASS | Workspace still type-checks; P01 changes are documentation and Python validation hardening. |
| `cargo test -p andromeda-procedure-contract --test procedure_contract_digest --locked` | PASS | Procedure contract digest and compatibility tests pass. |
| `cargo test -p andromeda-catalog --test catalog_digest_contract --locked` | PASS | Catalog digest contract tests pass. |
| `cargo test -p andromeda-catalog --test catalog_store_contract --locked` | PASS | Catalog durable publication and recovery contract tests pass. |
| `cargo test -p andromeda-wal-codec -p andromeda-wal --locked` | PASS | WAL codec, FileWAL, durability fence, recovery, GC, and frame contract tests pass. |
| `python -B tools/testing/roadmap_gate_summary.py --format json` | FAIL | Required `.github/workflows/*` quality and release workflows are absent. |
| `python -B tools/testing/validation_manifest.py --json` | BLOCKED | Expected release blockers remain: retained fuzz, Miri, B-Tree crash/recovery, backup/PITR/HA-DR drills, release evidence packet. |

## P01 Decision

P01 closure is complete for the normative specification baseline. The stage now has concrete V0 contracts, explicit rejection behavior, owner/evidence acceptance summaries, and a strict local gate.

The release gate remains closed. Do not claim production readiness until retained release evidence exists for fuzzing, Miri, backup/PITR restore drills, HA/DR cluster drills, B-Tree durable promotion crash/recovery, CI workflows, and final release evidence packets.

## P01 Doctrine/Spec Coherence Addendum

The reusable specification template and specification index now carry the P00 evidence governance rule forward into P01 closure:

- Specs must name source grounding, owner crates, acceptance evidence, and the highest C0-C5 criticality they touch.
- Acceptance summaries must include literal `Owner`, `Evidence`, and `Reject` entries.
- C4/C5 paths must identify retained evidence tied to doctrine invariants, not only generic green commands, demos, scaffolds, benchmarks, traces, or simulation-only drills.
- Specs touching durable state, external surfaces, security, admission, audit, backup, restore, HA/DR, WAL, MVCC, or catalog truth must define crash/recovery or fail-closed evidence before acceptance.
- The specification index explicitly blocks production-readiness or release-readiness claims without the separate release evidence packet and roadmap release gates.
- P01-specific individual-spec rejection entries and acceptance summaries have been remediated by their spec owners.

## P00 Result

P00 repository-state validation is locally coherent, but the repository is not release-ready.

| Gate | Result | Notes |
|---|---:|---|
| `python -B tools/testing/p00_repository_state_check.py --strict` | PASS | Repository-state checker confirms Rust 1.95.0, Edition 2024, resolver 3, 89 crates, required P00 deliverables, no stale current roadmap path, and no unbounded production-readiness claims. |
| `cargo metadata --no-deps --locked --format-version 1` | PASS | Workspace resolves with 89 crates. |
| `cargo deny check` | PASS | Completed with duplicate-version warnings that remain supply-chain cleanup work. |
| `cargo check --workspace --locked` | PASS | Workspace type-checks after P00 manifest corrections. |
| `cargo test -p andromeda-cli --locked --test workspace_dependency_topology -- --nocapture` | PASS | P00 topology gate passes with 32 tests, including Cargo-derived member parsing and declared-versus-physical crate checks. |
| `cargo clippy -p andromeda-cli --all-targets --locked -- -D warnings` | PASS | Focused Clippy gate for the crate touched by P00 topology code. |
| `cargo test --workspace --locked` | PASS | Full workspace tests pass. |
| `cargo clippy --workspace --locked --all-targets -- -D warnings` | PASS | Clippy-clean after scoped cleanup fixes. |
| `python -m py_compile tools/testing/p00_repository_state_check.py` | PASS | P00 checker compiles. |
| Targeted `cargo fmt ... -- --check` on touched crates | PASS | Rustfmt emits config warnings for nightly-only or unknown options. |
| `cargo fmt --all -- --check` | BLOCKED | Windows path length failure: `os error 206`. |
| `python -B tools/testing/backup_restore_drill_check.py --format json` | PASS | Read-only readiness check only; not production restore proof. |
| `python -B tools/testing/hadr_cluster_drill_check.py --format json` | PASS | Simulation-only readiness check only; not production failover proof. |
| `python -B tools/testing/crash_matrix_check.py --format json` | PASS | Required CR-11 scenarios are indexed with evidence commands. |
| `python -B tools/testing/validation_manifest.py --json` | BLOCKED | Expected release blockers remain: retained fuzz, Miri, B-Tree crash/recovery, backup/PITR/HA-DR drills, release evidence packet. |
| `python -B tools/testing/roadmap_gate_summary.py --format json` | FAIL | Required `.github/workflows/*` quality and release workflows are absent. |

## P00 Decision

P00 baseline/governance work is complete enough to proceed with the next roadmap discussion.

The release gate remains closed. Do not claim production readiness until retained release evidence exists for fuzzing, Miri, backup/PITR restore drills, HA/DR cluster drills, B-Tree durable promotion crash/recovery, CI workflows, and final release evidence packets.

## P00 Evidence Governance Addendum

P00 now treats the criticality model and feature acceptance gate as normative sources for phase evidence. A phase is not closed by crate existence, documentation presence, or a generic green workspace command alone. The validation report for each phase must tie the changed path to its C0-C5 criticality, the retained source artifact, the validation command or review record, and crash/recovery or fail-closed evidence when durable state, external surfaces, security, admission, audit, backup, restore, HA/DR, WAL, MVCC, or catalog truth is touched.

Production readiness remains an explicit blocked status unless the retained evidence packet is present and referenced. Local pass results, simulation-only drills, benchmarks, demos, scaffolds, and advisory evidence do not upgrade a path to production-ready status.
