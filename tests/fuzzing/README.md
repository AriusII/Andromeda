# Fuzzing Test Index

## Purpose

This directory is a roadmap index for fuzzing coverage. It does not own executable fuzz targets.

Fuzzing supports parser, codec, canonicalization, persisted byte, and state-machine hardening. Fuzz output is evidence for finding defects, not correctness truth for C4/C5 behavior.

## Scope

Use this index for fuzz targets that exercise untrusted input, persisted bytes, network frames, procedure contracts, SRPL parsing, binary codecs, and recovery records.

| Area | Owning validation location |
| --- | --- |
| Seed corpus and fuzz workspace | `fuzz/` |
| Fuzz validation matrix | `fuzz/VALIDATION_MATRIX.md` |
| SRPL parser and compiler properties | `crates/andromeda-srpl/tests/property_parser_fuzz.rs`, `crates/andromeda-srpl-parser/tests/*`, `crates/andromeda-srpl-ast/tests/*` |
| Storage, WAL, and recovery properties | `crates/andromeda-storage/tests/property_*`, `crates/andromeda-wal/tests/*` |
| RPC and wire codecs | `crates/andromeda-rpc-protocol/tests/frame_wire_contract.rs`, `crates/andromeda-quic/tests/codec_contract.rs` |

## Non-goals

- Do not add root-level fuzz harnesses here.
- Do not use fuzz success as a replacement for deterministic tests, crash/recovery gates, Miri, Loom, or release criteria.
- Do not fuzz by constructing ad hoc SQL or bypassing typed Procedure contracts.
- Do not keep generated corpora or large fuzz artifacts in this index.

## Prerequisites

- `cargo-fuzz` installed when running fuzz targets.
- Seed corpus generation available through `fuzz/generators/generate_seed_corpus.py`.
- A named invariant for each fuzz target.
- A deterministic regression test path for every minimized failure.

## Procedure

1. Identify the input surface and owning crate.
2. Define the invariant before running the fuzz target.
3. Keep the executable fuzz target in `fuzz/` or the owning crate's established test location.
4. Promote minimized failures to deterministic regression tests in the owning crate.
5. Update this index only with stable target names, owners, invariants, and validation commands.

## Acceptance Criteria

- Each fuzz entry names the input surface, owning crate, invariant, corpus source, and reproduction command.
- Persisted-byte and wire-format fuzzing uses explicit codecs and never relies on Rust native struct layout.
- Minimized failures become deterministic tests before the issue is considered closed.
- Fuzz output is not treated as durable state, release truth, or benchmark evidence.
- Coverage that touches C5 behavior links to the crash/recovery or security gate that remains authoritative.

## Validation

Recommended preflight:

```powershell
python fuzz/generators/generate_seed_corpus.py --check
cargo check --manifest-path fuzz/Cargo.toml --locked
```

Run target-specific fuzz commands from the owning fuzz workspace after the target name is defined.

## Troubleshooting

If a fuzz target finds a crash that cannot be minimized, preserve the input only as temporary investigation evidence and create a deterministic reproduction before closing the defect.

If a fuzz target depends on timing, external services, GPU output, or hidden process state, redesign the target around a deterministic codec, parser, or state transition boundary.

## References

- `tests/README.md`
- `fuzz/VALIDATION_MATRIX.md`
- `docs/codex/rust-critical-quality-gates.md`
- `documentations/testing/step-11-validation-matrix.md`
