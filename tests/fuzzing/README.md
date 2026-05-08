# Fuzzing Test Index

## Purpose

This directory is an index only. The executable fuzz source of truth remains `fuzz/`, including target sources, target registry, corpus manifest, deterministic seed generator, and validation matrix.

Fuzzing evidence helps find parser, codec, persisted-byte, wire-format, and state-machine defects. It is not durable truth for C4/C5 behavior and does not replace deterministic tests, crash/recovery validation, Miri, Loom, or release gates.

## Scope

Use this index to find the owning fuzz artifacts:

| Artifact | Source of truth |
| --- | --- |
| Fuzz workspace and executable targets | `fuzz/` |
| Target registry | `fuzz/targets.toml` |
| Cargo fuzz binaries | `fuzz/Cargo.toml` |
| Deterministic seed corpus manifest | `fuzz/corpus/manifest.toml` |
| Seed corpus generator and checker | `fuzz/generators/generate_seed_corpus.py` |
| Fuzz coverage matrix | `fuzz/VALIDATION_MATRIX.md` |
| Registry and CI command-shape checker | `tools/testing/fuzz_registry_check.py` |
| Fuzz CI workflow | `.github/workflows/07-fuzzing.yml` |

## Non-goals

- Do not add executable fuzz harnesses under `tests/fuzzing/`.
- Do not migrate, copy, or regenerate the corpus from this directory.
- Do not duplicate `fuzz/targets.toml`, `fuzz/Cargo.toml`, or `fuzz/corpus/manifest.toml` here.
- Do not use fuzz success as a substitute for deterministic regression tests or crash/recovery evidence.
- Do not fuzz by constructing ad hoc SQL or bypassing typed Procedure contracts.

## Validation

Run these checks from the repository root when fuzz registry or corpus metadata changes:

```powershell
python tools/testing/fuzz_registry_check.py
python fuzz/generators/generate_seed_corpus.py --check
```

The registry checker is read-only. It verifies that `fuzz/targets.toml`, `fuzz/Cargo.toml`, `fuzz/corpus/manifest.toml`, `fuzz/fuzz_targets/*.rs`, tracked deterministic seeds, and `.github/workflows/07-fuzzing.yml` command shape remain aligned.

## References

- `tests/README.md`
- `fuzz/README.md`
- `fuzz/VALIDATION_MATRIX.md`
- `docs/codex/rust-critical-quality-gates.md`
- `documentations/testing/step-11-validation-matrix.md`
