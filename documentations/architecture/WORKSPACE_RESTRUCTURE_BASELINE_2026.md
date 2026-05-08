# Workspace Restructure Baseline 2026

## Purpose

This document records the current baseline used to continue the workspace and crate restructure roadmap. It separates proven committed progress from the current dirty worktree so future packets can be packaged and validated without losing traceability.

## Scope

This baseline applies to the Andromeda workspace on branch `codex/workspace-crate-restructure` after the Lot 5 commits that introduced runtime-free RPC protocol contracts, security contract boundaries, and fuzz/admission documentation.

It covers the root workspace layout, crate ownership status, migration risk, and minimum validation gates for the next restructure packets.

## Non-goals

This document does not declare the current working tree release-ready. It does not replace ADR-0011, the external roadmap, or domain-specific specs. It also does not approve broad C5 storage, WAL, recovery, transaction, or HA/DR extraction without targeted crash/recovery validation.

## Prerequisites

Before using this baseline for a commit or PR, verify:

- The worktree has no unintended `AD` entries.
- The staged index matches the intended packet.
- The packet is scoped to one responsibility boundary.
- The packet has a targeted validation command.
- The packet does not weaken Andromeda invariants.

## Baseline

The external roadmap was written against the original `main` branch shape, where the workspace had 11 primary crates. The current restructure branch has already moved beyond that baseline and includes 26 workspace members.

Crates already present include the original broad crates plus these extracted or newly introduced crates:

- `andromeda-contract`
- `andromeda-digest`
- `andromeda-error`
- `andromeda-hardware`
- `andromeda-rpc-protocol`
- `andromeda-security-contract`
- `andromeda-srpl-ast`
- `andromeda-srpl-cardinality`
- `andromeda-srpl-diagnostics`
- `andromeda-srpl-ir`
- `andromeda-srpl-parser`
- `andromeda-structured-object`
- `andromeda-time`
- `andromeda-types`
- `andromeda-wal`

This means the next work is not a greenfield split. It is a continuation of an active migration with compatibility facades, dirty staged state, and several partially extracted responsibilities.

## Current Status

The restructure is advanced in these areas:

- Foundation crates exist for errors, types, digest, time, hardware, and compatibility through `andromeda-core`.
- Contract and StructuredObject ownership have moved into dedicated crates.
- SRPL parser, AST, diagnostics, cardinality, and IR have partial owner crates.
- WAL primitives and FileWal ownership have moved into `andromeda-wal`.
- RPC frame and ResultStream protocol contracts have moved into `andromeda-rpc-protocol`.
- Security admission vocabulary has moved into `andromeda-security-contract`.
- Lot 5 fuzz targets and protocol/security docs exist.
- ADR-0011 documents crate rings, temporary exceptions, and topology gates.

The restructure remains partial in these areas:

- `andromeda-core` is still imported by several high-risk crates.
- `andromeda-storage`, `andromeda-catalog`, `andromeda-exec`, `andromeda-tx`, `andromeda-quic`, `andromeda-observe`, `andromeda-srpl`, `andromeda-proto`, and `andromeda-bench` are still broad owners.
- Several roadmap crates do not exist yet, including `andromeda-codec`, `andromeda-policy`, `andromeda-resource`, `andromeda-definition-batch`, `andromeda-proto-wire`, `andromeda-admission`, `andromeda-result-stream`, `andromeda-procedure-store`, `andromeda-recovery`, `andromeda-statistics`, `andromeda-optimizer`, `andromeda-plan-cache`, `andromeda-maps`, and the benchmark split crates.
- Several P0 specs are still being produced or remain pending.

## Procedure

Use this packet order for the next restructure work:

1. Stabilize staging and remove contradictory `AD` states from any packet before commit.
2. Complete root tooling and validation metadata.
3. Finish P0 specs and ADR backlog before changing C5 runtime behavior.
4. Harden policy, topology, and supply-chain gates.
5. Extract low-risk non-C5 owners first, such as SRPL lexer/lowering, benchmark workload/harness, and statistics/plan-cache scaffolds.
6. Prepare C5 extraction with behavior locks before moving WAL, transaction, storage, recovery, backup, restore, or HA/DR boundaries.
7. Validate every packet with the strongest targeted gate before broad workspace gates.

## Validation

Use these baseline validation commands after staging is coherent:

```powershell
python .github/scripts/andromeda_policy_gate.py --root .
python .codex/scripts/validate_codex_tooling.py
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture
cargo fmt --all -- --check
cargo check --workspace --locked
```

For C5 packets, add the relevant WAL, transaction, storage, recovery, backup/restore, or HA/DR tests before any merge.

## Troubleshooting

If a packet cannot be validated because the worktree is too dirty, reduce the packet. Do not broaden staging to make a command pass. Use path-specific diffs and path-specific commits.

If staged and working-tree content disagree for the same file, inspect both versions before changing anything. Do not assume the staged version is newer or safer.

If a document claims a behavior is implemented, verify the claim against code or tests in the same packet or mark it as planned.

## References

- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `documentations/CURRENT_STATE.md`
- `documentations/ROADMAP_IMPLEMENTATION_2026.md`
- `documentations/WORKER_EXECUTION_MATRIX_2026.md`
- `C:/Users/Arius/Desktop/andromeda_roadmap_restructuration_workspace_crates_engines_2026.md`
