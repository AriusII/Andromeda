# Fuzzing Test Index

## Purpose

This directory is an index only. Executable fuzzing assets remain in `fuzz/`, while this directory owns orchestration documentation and test-indexing references.

Fuzzing evidence helps find parser, codec, persisted-byte, wire-format, and state-machine defects. It is not durable truth for C4/C5 behavior and does not replace deterministic tests, crash/recovery validation, Miri, Loom, or release gates.

## Scope

Use this index to find the owning fuzz artifacts:

| Artifact | Source of truth |
| --- | --- |
| Fuzz workspace and executable targets | `fuzz/` |
| Orchestration docs and index surface | `tests/fuzzing/` |
| Target registry | `tests/fuzzing/targets.toml` |
| Cargo fuzz binaries | `fuzz/Cargo.toml` |
| Deterministic seed corpus manifest | `tests/fuzzing/corpus/manifest.toml` |
| Seed corpus generator and checker | `fuzz/generators/generate_seed_corpus.py` |
| Fuzz coverage matrix | `fuzz/VALIDATION_MATRIX.md` |
| Registry and CI command-shape checker | `tools/testing/fuzz_registry_check.py` |
| Fuzz CI workflow | `.github/workflows/07-fuzzing.yml` |
| Release smoke workflow | `.github/workflows/release-gate-chain.yml` |

## Non-goals

- Do not add executable fuzz harnesses under `tests/fuzzing/`.
- Do not migrate, copy, or regenerate the corpus from this directory.
- Do not duplicate `tests/fuzzing/targets.toml`, `fuzz/Cargo.toml`, or `tests/fuzzing/corpus/manifest.toml`.
- Do not use fuzz success as a substitute for deterministic regression tests or crash/recovery evidence.
- Do not fuzz by constructing ad hoc SQL or bypassing typed Procedure contracts.

## Quick launch (CI-shape smoke, 15s default)

```bash
python tools/testing/fuzz_registry_check.py \
  && python fuzz/generators/generate_seed_corpus.py --check \
  && python tools/testing/fuzz_registry_check.py --list-targets \
  | while IFS=$'\t' read -r target corpus_dir; do \
      cargo +nightly fuzz run "$target" "$corpus_dir" -- -max_total_time=15; \
    done
```

This command is consistent with the `07-fuzzing` workflow shape (`--check`, `--list-targets`, then target runs).
`release-gate-chain` should use the same registry discovery shape for its 15-second smoke runs.

## Validation

Run these checks from the repository root when fuzz registry or corpus metadata changes:

```powershell
python tools/testing/fuzz_registry_check.py
python fuzz/generators/generate_seed_corpus.py --check
```

The registry checker is read-only. It verifies that `tests/fuzzing/targets.toml`, `fuzz/Cargo.toml`, `tests/fuzzing/corpus/manifest.toml`, `fuzz/fuzz_targets/*.rs`, tracked deterministic seeds, and `.github/workflows/07-fuzzing.yml` command shape remain aligned.

## References

- `tests/README.md`
- `fuzz/README.md`
- `fuzz/VALIDATION_MATRIX.md`
- `docs/codex/rust-critical-quality-gates.md`
- `documentations/testing/step-11-validation-matrix.md`
