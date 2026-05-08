# Andromeda Worktree Packaging Plan - 2026-05-08

## Purpose

Define a concrete packaging plan for the current dirty Andromeda worktree so multiple workers can package disjoint changes without overwriting, reverting, unstaging, staging, or committing work they do not own.

This plan treats the current index as unsafe for direct commit. Every package must be built from explicit pathspecs after the owner has reconciled staged and working-tree content for those paths.

## Scope

This document applies to packaging work in `C:/Users/Arius/RustroverProjects/Andromeda` as observed on 2026-05-08.

The worktree status snapshot used for this plan, before this plan file was added, is:

| Status | Count | Meaning |
| --- | ---: | --- |
| `MM` | 282 | Path has staged content and different unstaged content. |
| `AD` | 63 | Path is added in the index and deleted in the working tree. |
| ` M` | 55 | Path has unstaged content only. |
| `A ` | 11 | Path is added in the index only. |
| `??` | 6 | Untracked path or directory is present. |

Primary `MM` concentration:

| Area | `MM` count | Packaging implication |
| --- | ---: | --- |
| `crates/andromeda-catalog` | 97 | Must be split into catalog subpackages; do not commit all catalog paths together by accident. |
| `crates/andromeda-proto` | 52 | Must be validated with protocol and generated-validation gates before commit. |
| `crates/andromeda-srpl` | 51 | Must be kept separate from SRPL AST, IR, parser, and cardinality crates unless a packet explicitly owns the boundary. |
| `crates/andromeda-observe` | 50 | Must be isolated from protocol and catalog packets except for declared audit/correlation contracts. |
| `crates/andromeda-bench` | 15 | Must remain evidence and benchmark packaging, not runtime truth. |

Untracked entries observed before this plan file was added:

- `.cargo/`
- `.config/`
- `documentations/architecture/`
- `rust-toolchain.toml`
- `rustfmt.toml`
- `tools/`

## Non-goals

This plan does not authorize any worker to:

- Run `git reset`, `git checkout`, `git restore`, `git clean`, or any equivalent destructive command.
- Unstage paths owned by another worker.
- Stage broad directories without an explicit packet pathspec.
- Commit from the current global index.
- Resolve `AD` or `MM` paths by discarding either side without owner review.
- Move changes between packets by editing outside the worker's declared ownership.

## Prerequisites

Before any worker starts a packet:

1. Confirm the worker owns every path it will edit.
2. Record the packet letter, packet name, owner, and exact pathspecs.
3. Run only read-only Git inspection commands until the packet owner has a reconciliation decision.
4. Treat the current index as contaminated by other workers.
5. Use explicit pathspecs for every diff, validation, stage, and commit action.

Recommended read-only inspection commands:

```powershell
git status --porcelain=v1 -- <pathspec>
git diff -- <pathspec>
git diff --cached -- <pathspec>
git diff --name-status -- <pathspec>
git diff --cached --name-status -- <pathspec>
```

## Do-Not-Commit-Index Warning

Do not run `git commit` against the current index.

The index already contains staged additions and modifications from multiple logical packets. A plain `git commit`, a broad `git add .`, or a directory-level commit from the repository root can mix unrelated workers, preserve stale staged content, or commit files that are deleted in the working tree.

Allowed commit shape after a packet is reconciled:

```powershell
git status --porcelain=v1 -- <packet-pathspecs>
git diff --check -- <packet-pathspecs>
git diff --cached --check -- <packet-pathspecs>
git add -- <packet-pathspecs>
git commit --only -- <packet-pathspecs>
```

If the packet contains paths already staged by another worker, stop and ask for packet ownership confirmation before using `git add` or `git commit --only`.

## Pathspec-Only Commit Strategy

Every packet must use pathspec-only staging and committing.

Rules:

1. Use `git add -- <exact-file>` for individual files or the narrowest safe directory.
2. Use `git commit --only -- <exact-file-or-directory>` to prevent unrelated staged paths from entering the commit.
3. Never use `git add .`, `git add -A`, `git commit -a`, or plain `git commit`.
4. Before commit, compare staged and unstaged views for the same pathspec.
5. After commit, confirm that only the packet's paths changed state.

Verification commands for a packet:

```powershell
git diff --name-status -- <packet-pathspecs>
git diff --cached --name-status -- <packet-pathspecs>
git status --porcelain=v1 -- <packet-pathspecs>
```

Repository-level safety check after each packet:

```powershell
git status --porcelain=v1
```

Use the repository-level status only to detect accidental cross-packet movement. Do not treat it as a commit input.

## Resolving Staged-vs-Working Divergence

`MM` and `AD` paths require explicit reconciliation before packaging.

For `MM` paths:

1. Inspect the staged side with `git diff --cached -- <path>`.
2. Inspect the working-tree side with `git diff -- <path>`.
3. Classify the staged side as one of:
   - belongs in this packet;
   - belongs in another packet;
   - stale and must be superseded by the working-tree side;
   - unresolved and requires owner review.
4. If the working-tree side is intended, edit the path to the final desired content and stage only that path.
5. If both sides contain needed work, merge them manually in the working file, then stage only that path.
6. If either side belongs to another worker, leave it untouched and exclude the path from this packet.

For `AD` paths:

1. Treat `AD` as a high-risk contradiction: the index says "add this file", while the working tree says "delete it."
2. Inspect the staged file with `git show :<path>` only as read-only evidence.
3. Decide whether the intended final state is present file, deleted file, or moved file.
4. If the intended final state is present, re-create the file content under the same path only when the packet owner owns that path.
5. If the intended final state is deleted, confirm that all references and module declarations in the same packet are updated.
6. If the intended final state is moved, package the move with both source and destination pathspecs.
7. Do not use reset, checkout, restore, or clean to resolve the contradiction.

For untracked paths:

1. Include only untracked files that are explicitly owned by the packet.
2. Do not include untracked directories wholesale unless the packet owns the entire directory tree.
3. Check generated files against `.gitignore` and the relevant package policy before staging.

## Current `AD` Risk Register

The following paths are staged as added but deleted in the working tree. They must not be committed until each owner decides whether the final packet should preserve, delete, or move the file.

| Area | `AD` paths |
| --- | --- |
| Bench evidence | `crates/andromeda-bench/src/benchmark_history/stats.rs` |
| Catalog contracts and modelization | `crates/andromeda-catalog/src/contracts/hash.rs`<br>`crates/andromeda-catalog/src/contracts/materialization.rs`<br>`crates/andromeda-catalog/src/contracts/types.rs`<br>`crates/andromeda-catalog/src/contracts/validation.rs`<br>`crates/andromeda-catalog/src/objects/shape_hash.rs`<br>`crates/andromeda-catalog/src/objects/validation.rs`<br>`crates/andromeda-catalog/src/plan_cache/evidence.rs` |
| CLI administration test | `crates/andromeda-cli/tests/cli_admin_commands/backup_restore.rs` |
| Core facade and hardware types | `crates/andromeda-core/src/digest.rs`<br>`crates/andromeda-core/src/error.rs`<br>`crates/andromeda-core/src/hardware_cpu.rs`<br>`crates/andromeda-core/src/hardware_gpu.rs`<br>`crates/andromeda-core/src/hardware_integration.rs`<br>`crates/andromeda-core/src/hardware_pipeline.rs`<br>`crates/andromeda-core/src/hardware_ram.rs`<br>`crates/andromeda-core/src/ids.rs`<br>`crates/andromeda-core/src/time.rs`<br>`crates/andromeda-core/src/types.rs` |
| Exec result and observed store | `crates/andromeda-exec/src/business/product_stock/observed_store.rs`<br>`crates/andromeda-exec/src/result/backpressure.rs`<br>`crates/andromeda-exec/src/result/frames.rs` |
| Proto generated validation and structured hash | `crates/andromeda-proto/src/generated_validation/runtime_projection/frame_result_metadata/invocation_correlation.rs`<br>`crates/andromeda-proto/src/structured/hash.rs`<br>`crates/andromeda-proto/tests/wire/payload_and_envelope_contract/generated_projection.rs`<br>`crates/andromeda-proto/tests/wire/payload_and_envelope_contract/generated_validation.rs` |
| SRPL split modules | `crates/andromeda-srpl/src/ir/evidence.rs`<br>`crates/andromeda-srpl/src/ir/plan.rs`<br>`crates/andromeda-srpl/src/ir/procedure.rs`<br>`crates/andromeda-srpl/src/ir/validation.rs`<br>`crates/andromeda-srpl/src/ir/values.rs`<br>`crates/andromeda-srpl/src/parser/core.rs`<br>`crates/andromeda-srpl/src/parser/helpers.rs`<br>`crates/andromeda-srpl/src/parser/statements.rs`<br>`crates/andromeda-srpl/src/parser/types.rs`<br>`crates/andromeda-srpl/src/source_location/forbidden_scan.rs` |
| Storage and WAL modules | `crates/andromeda-storage/src/backup/execution_plan/failure.rs`<br>`crates/andromeda-storage/src/btree/node_format_v1/golden.rs`<br>`crates/andromeda-storage/src/btree_key_codec/golden.rs`<br>`crates/andromeda-storage/src/write_ahead_log/commit_log_entry/error.rs`<br>`crates/andromeda-storage/src/write_ahead_log/commit_log_facade/facade.rs`<br>`crates/andromeda-storage/src/write_ahead_log/commit_log_facade/tests.rs`<br>`crates/andromeda-storage/src/write_ahead_log/durability_fence/error.rs`<br>`crates/andromeda-storage/src/write_ahead_log/durability_fence/validation.rs`<br>`crates/andromeda-storage/src/write_ahead_log/gc_eligibility/checker.rs`<br>`crates/andromeda-storage/src/write_ahead_log/gc_eligibility/result.rs`<br>`crates/andromeda-storage/src/write_ahead_log/record_bounds/batch.rs`<br>`crates/andromeda-storage/src/write_ahead_log/record_bounds/cardinality.rs`<br>`crates/andromeda-storage/src/write_ahead_log/record_bounds/constants.rs`<br>`crates/andromeda-storage/src/write_ahead_log/record_bounds/errors.rs`<br>`crates/andromeda-storage/src/write_ahead_log/record_bounds/lsn.rs`<br>`crates/andromeda-storage/src/write_ahead_log/record_bounds/segment.rs`<br>`crates/andromeda-storage/src/write_ahead_log/record_bounds/size.rs`<br>`crates/andromeda-storage/src/write_ahead_log/segment_reclaimability/validation.rs` |
| Transaction WAL helpers | `crates/andromeda-tx/src/commit_log/manager/replay_reconstruction.rs`<br>`crates/andromeda-tx/src/wal_adapter/test_helpers.rs` |
| Legacy docs migration | `docs/00_ANDROMEDA_INDEX_ET_MODE_DE_LECTURE.md`<br>`docs/01_DOCTRINE_LEXIQUE_ARCHITECTURE_CATALOGUE_MODELIZATION.md`<br>`docs/02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md`<br>`docs/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`<br>`docs/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md`<br>`docs/05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md`<br>`docs/ANDROMEDA_ROADMAP_IMPLEMENTATION_CROSSCHECK_2026.md` |

## Packet Order A-J

Package in the following order. Each packet must own only its pathspecs. If a worker finds a dependency on a later packet, record it as a blocker instead of widening ownership.

| Packet | Name | Primary pathspecs | Required owner decision |
| --- | --- | --- | --- |
| A | Repository policy and workspace scaffolding | `.github/scripts/andromeda_policy_gate.py`, `.github/scripts/tests/test_andromeda_policy_gate.py`, `.gitignore`, `Cargo.lock`, `crates/README.md`, `rust-toolchain.toml`, `rustfmt.toml`, `.cargo/`, `.config/`, `tools/` | Decide whether untracked toolchain and tooling directories are intentional project files or local environment residue. |
| B | Documentation migration and operations docs | `docs/`, `documentations/`, excluding this packaging plan unless intentionally updated by the documentation owner | Resolve `docs/` `AD` files before committing any documentation migration. |
| C | Core facade, contract crate, and primitive identifiers | `crates/andromeda-core/`, `crates/andromeda-contract/`, `crates/andromeda-types/`, `crates/andromeda-time/` | Resolve core `AD` facade files before committing lib reexports or compatibility tests. |
| D | Protocol, structured payloads, RPC protocol, and QUIC gateway | `crates/andromeda-proto/`, `crates/andromeda-rpc-protocol/`, `crates/andromeda-quic/` | Resolve generated-validation `AD` paths and confirm no gRPC or JSON runtime surface was introduced. |
| E | Catalog contracts, DefinitionBatch, plan evidence, and statistics | `crates/andromeda-catalog/` | Split catalog subareas if needed; reconcile catalog contract `AD` paths before committing any facade references. |
| F | SRPL language, parser, binder, IR, and related crates | `crates/andromeda-srpl/`, `crates/andromeda-srpl-ast/`, `crates/andromeda-srpl-cardinality/`, `crates/andromeda-srpl-ir/`, `crates/andromeda-srpl-parser/` | Confirm split-module `AD` files are either restored, replaced by new crate paths, or deleted with all module references updated. |
| G | Execution surface and IAM/audit integration | `crates/andromeda-exec/`, `crates/andromeda-cli/` | Keep Administration and HA/DR outside the Application Surface; reconcile exec result `AD` files and CLI admin test deletion. |
| H | Observability and durable audit | `crates/andromeda-observe/` | Validate audit family contracts and principal binding without coupling to unrelated proto or catalog changes. |
| I | Storage, WAL, transaction, hardware, and structured object | `crates/andromeda-storage/`, `crates/andromeda-tx/`, `crates/andromeda-hardware/`, `crates/andromeda-structured-object/` | Resolve storage/WAL `AD` files before any crash/recovery validation claim. |
| J | Benchmarks and scenario evidence | `crates/andromeda-bench/` | Keep benchmark output advisory only; resolve benchmark history `AD` file before publishing benchmark evidence changes. |

## Validation

Use the smallest validation gate that proves the packet. Broaden only when the packet crosses engine boundaries.

| Packet | Minimum validation |
| --- | --- |
| A | `python3 .codex/scripts/validate_codex_tooling.py` when Codex tooling is touched; targeted policy gate tests for `.github/scripts`; `cargo fmt --all --check` if Rust formatting files are packaged. |
| B | Markdown review, link/path inspection, and `python3 .codex/scripts/validate_codex_tooling.py` if Codex documentation or agent operations changed. |
| C | `cargo fmt --all --check`; `cargo check -p andromeda-core -p andromeda-contract -p andromeda-types -p andromeda-time --all-targets --all-features`; targeted facade compatibility tests. |
| D | `cargo fmt --all --check`; `cargo check -p andromeda-proto -p andromeda-rpc-protocol -p andromeda-quic --all-targets --all-features`; protocol contract tests; no-gRPC and no-runtime-JSON scans. |
| E | `cargo fmt --all --check`; `cargo check -p andromeda-catalog --all-targets --all-features`; DefinitionBatch, catalog publication, plan cache, and WAL record tests touched by the packet. |
| F | `cargo fmt --all --check`; `cargo check` for all SRPL crates; parser, binder, cardinality, optimizer, and DefinitionBatch compatibility tests touched by the packet. |
| G | `cargo fmt --all --check`; `cargo check -p andromeda-exec -p andromeda-cli --all-targets --all-features`; IAM, permission audit, application-surface, and admin-surface tests touched by the packet. |
| H | `cargo fmt --all --check`; `cargo check -p andromeda-observe --all-targets --all-features`; audit family, durable audit, sequence, sink, and correlation tests touched by the packet. |
| I | `cargo fmt --all --check`; `cargo check` for storage, transaction, hardware, and structured-object crates; add crash/recovery, WAL replay, Miri, fuzz, or property tests when durability paths are touched. |
| J | `cargo fmt --all --check`; `cargo check -p andromeda-bench --all-targets --all-features`; targeted benchmark evidence and scenario boundary tests. |

Global release-level validation after all packets:

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
cargo test --doc --workspace
cargo audit
cargo deny check
```

For C4/C5 storage, WAL, recovery, security, RPC, or catalog changes, require targeted crash/recovery, fuzz, Miri, or property tests before declaring mission-critical readiness.

## Troubleshooting

### A packet sees unrelated staged paths

Stop packaging that packet. Use read-only inspection to identify the unrelated paths, then continue only with `git commit --only -- <packet-pathspecs>` after the packet's own paths are reconciled.

### A packet path is `MM`

Do not assume the staged version is older or newer. Compare both sides, merge intentionally in the working file if needed, then stage only that path.

### A packet path is `AD`

Do not commit until the owner decides whether the final state is present, deleted, or moved. An `AD` path can otherwise publish a file that no longer exists locally or drop a file that another packet expects.

### A validation gate fails outside the packet

Record the failure and its owning path. Do not edit outside the packet to make a global gate pass. Either narrow the validation to packet-owned tests or hand off the failure to the owning worker.

### CRLF/LF warnings appear

Treat line-ending churn as a packaging risk. Do not run broad format or normalization commands unless the packet owns every affected file. If a formatter changes unrelated files, stop and discard only the formatter output that the packet owner created, without reverting user work.

## References

- Root `AGENTS.md` invariants for Andromeda.
- `git status --porcelain=v1` snapshot collected on 2026-05-08.
- `git diff --cached --name-status` and `git diff --name-status` inspection for staged-vs-working divergence.
