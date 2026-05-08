# Fixture Test Index

## Purpose

This directory is a documentation index for test fixture governance. It does not own executable fixtures, generators, or harnesses.

Fixture truth remains with the owning crate, checked-in corpus, golden vector, or deterministic generator that is validated by an explicit command.

## Scope

Use this index for roadmap entries that refer to `tests/fixtures`.

Entries should name the owning crate or corpus, fixture purpose, determinism rule, regeneration policy, and validation command.

## Fixture destination

| Fixture type | Destination |
| --- | --- |
| Crate-specific test data, builders, mocks, or golden vectors | The owning crate's `tests/`, `src/tests`, or documented fixture module. |
| Shared deterministic helpers used by multiple crates | A dedicated test-support crate with explicit ownership and validation commands. |
| Fuzz seed corpus, minimization output, or target metadata | `fuzz/`, `tests/fuzzing/corpus/`, `tests/fuzzing/targets.toml`, and the `tests/fuzzing/` index. |
| Generated fixtures | The owning generator and checked validation command; do not commit generated output here unless a future work order defines that corpus. |
| Roadmap-only fixture coverage | This README, as an index entry that points to the owner and regeneration policy. |

## Non-goals

- Do not add large generated outputs to this directory.
- Do not add executable fixture builders without an explicit future work order.
- Do not duplicate crate-local fixtures, fuzz targets, seed corpora, or generated corpora in this directory.
- Do not use fixture output as a substitute for deterministic assertions, crash/recovery evidence, or security validation.
- Do not create fixtures that bypass typed Procedure contracts or persisted-format codecs.

## Prerequisites

- Review `tests/README.md`.
- Identify whether the fixture belongs in an owning crate, `fuzz/`, or another existing corpus location.

## Procedure

1. Map the fixture to the owning validation surface.
2. Keep executable generators and fixture-consuming tests with the owner.
3. Route fuzz seeds and target metadata to `fuzz/` and cite them through `tests/fuzzing/` instead of copying them here.
4. Record the fixture owner, regeneration command, review rule, and validation command in this index when the fixture is ready.

## Validation

For this documentation index, run:

```powershell
rg -n "Purpose|Scope|Validation" tests/fixtures
```

Runtime validation belongs to the owning crate, corpus checker, fuzz target, or golden-vector test command.

## Troubleshooting

If a fixture cannot be regenerated deterministically, keep it out of this index until its owner and validation command are defined.

## References

- `tests/README.md`
- `tests/fuzzing/README.md`
- `fuzz/VALIDATION_MATRIX.md`
- `docs/codex/rust-critical-quality-gates.md`
