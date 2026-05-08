# Storage Test Index

## Purpose

This directory is a documentation index for storage validation. It does not own an executable Rust harness.

Executable storage tests remain with the crates that own pages, extents, segments, manifests, BufferPool behavior, HotStore and ColdStore placement, WAL integration, backup, restore, and recovery behavior.

## Scope

Use this index for roadmap entries that refer to `tests/storage`.

Entries should name the owning crate, storage layer, persisted format or invariant, failure mode, and validation command.

## Test destination

| Scenario type | Destination |
| --- | --- |
| Page, heap, B-tree, extent, segment, manifest, or persisted binary format behavior | The owning storage crate's `tests/` directory or crate-local unit tests. |
| WAL integration, durability fence, replay, transaction log, or rollback behavior | The owning WAL, storage, or transaction crate's tests. |
| BufferPool, HotStore, ColdStore, backup, restore, or HA/DR storage behavior | The crate that owns the storage boundary being asserted. |
| Corruption, partial-write, or golden-vector regression | The owning codec or storage crate's deterministic tests, with golden vectors kept beside the owner. |
| Fuzz-discovered persisted-byte or codec defect | A deterministic crate-local regression; keep fuzz target and corpus metadata in `fuzz/` and index them through `tests/fuzzing/`. |
| Root roadmap storage coverage | This README, as an index entry that points to the owning crate command. |

## Non-goals

- Do not serialize Rust native structs directly to disk or network.
- Do not treat temp storage, RAM, benchmark output, or generated fixture output as truth.
- Do not create root-level storage harnesses without an explicit future work order.
- Do not duplicate crate-local storage tests, fuzz targets, seed corpora, or generated fixtures in this directory.
- Do not weaken WAL, manifest, recovery, or visibility invariants.

## Prerequisites

- Review `tests/README.md`.
- Identify the owning storage, WAL, transaction, catalog, or backup crate before adding executable tests elsewhere.

## Procedure

1. Map the storage scenario to the owning crate.
2. Keep executable storage tests in that crate.
3. Convert fuzz-discovered persisted-byte failures into deterministic codec, corruption, or replay regressions before citing them here.
4. Record the persisted evidence, crash or corruption expectation, and validation command in this index when the scenario is ready.

## Validation

For this documentation index, run:

```powershell
rg -n "Purpose|Scope|Validation" tests/storage
```

Runtime validation belongs to the owning storage, WAL, transaction, catalog, backup, or recovery crate test command.

## Troubleshooting

If a storage scenario depends on recovery replay, link the corresponding recovery entry instead of duplicating executable assets here.

## References

- `tests/README.md`
- `tests/crash-recovery/README.md`
- `docs/codex/rust-critical-quality-gates.md`
- `documentations/testing/step-11-validation-matrix.md`
