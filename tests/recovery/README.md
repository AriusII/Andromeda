# Recovery Test Index

## Purpose

This directory is a documentation index for recovery validation. It does not own an executable Rust harness.

Recovery truth remains with the crate-owned suites for WAL durability, replay, transaction visibility, catalog publication, storage manifests, and executor recovery behavior.

## Scope

Use this index for roadmap entries that refer to `tests/recovery`.

Entries should name the owning subsystem, crash point, durable evidence, replay expectation, and post-recovery visibility assertion.

## Test destination

| Scenario type | Destination |
| --- | --- |
| WAL record, segment, checksum, scan, or replay behavior | The owning WAL crate's `tests/` directory or crate-local unit tests. |
| Storage crash replay, manifest publication, page recovery, or corruption handling | The owning storage crate's `tests/` directory. |
| Transaction durability, rollback, visibility, or WAL adapter behavior | The owning transaction crate's `tests/` directory. |
| Catalog, map, or executor recovery visibility | The crate that owns the recovered state or visibility boundary. |
| Fuzz-discovered recovery defect | A deterministic crate-local crash/replay regression; keep fuzz target and corpus metadata in `fuzz/` and index them through `tests/fuzzing/`. |
| Root roadmap recovery coverage | This README or `tests/crash-recovery/README.md`, as an index entry only. |

## Non-goals

- Do not duplicate the existing `tests/crash-recovery/` index.
- Do not create a root-level recovery harness without an explicit future work order.
- Do not duplicate crate-local recovery tests, fuzz targets, seed corpora, or generated fixtures in this directory.
- Do not accept RAM, temporary files, fuzz output, or benchmark output as durable truth.
- Do not weaken the WAL-before-visible-commit invariant.

## Prerequisites

- Review `tests/README.md` and `tests/crash-recovery/README.md`.
- Identify the owning crate and deterministic crash point before adding any executable test elsewhere.

## Procedure

1. Map the recovery scenario to the owning crate.
2. Keep executable recovery tests in that crate.
3. Convert fuzz-discovered persisted-byte failures into deterministic crash/replay or corruption tests before citing them here.
4. Record durable evidence, replay expectations, and validation commands in this index when the scenario is ready.

## Validation

For this documentation index, run:

```powershell
rg -n "Purpose|Scope|Validation" tests/recovery
```

Runtime validation belongs to the owning WAL, storage, transaction, catalog, map, or executor crate test command.

## Troubleshooting

If a recovery scenario cannot name a deterministic crash point and replay assertion, treat it as design work rather than a ready test entry.

## References

- `tests/README.md`
- `tests/crash-recovery/README.md`
- `docs/codex/mission-critical-change-policy.md`
- `docs/codex/rust-critical-quality-gates.md`
