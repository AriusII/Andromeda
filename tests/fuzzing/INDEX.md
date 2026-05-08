# Fuzzing Index

## Purpose

This document is the canonical registry of executable fuzzing targets for Andromeda.

It is the test-side source of truth for:

- registered targets,
- seed corpus entries per target,
- target property policy (`invalid_input_policy`),
- execution status.

## Scope

Canonical definitions are:

- `fuzz/`: fuzz targets, support modules, build manifests, and tooling scripts.
- `tests/fuzzing/targets.toml`: canonical target registry.
- `tests/fuzzing/corpus/manifest.toml`: declared seed list by target.
- `tests/fuzzing/corpus/`: committed seed files.
- `fuzz/Cargo.toml` is not the source of truth for seed declarations.

## Non-goals

- Add executable harnesses outside `fuzz/`.
- Change the execution policy to bypass `cargo-fuzz`.
- Treat fuzz success as evidence for WAL durability, recovery, global security, or feature completeness.

## Prerequisites

- Run checks and fuzz runs from `fuzz/`.
- Use `tests/fuzzing/targets.toml` as the target registry.
- Use `tests/fuzzing/corpus/manifest.toml` as the seed manifest.

## Fuzz target index (detailed)

Reference state as of 2026-05-08.

| Target | Corpus | Seeds | Property (`invalid_input_policy`) | Status | validation_date | owner | next_action |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `frame_codec_no_panic` | `tests/fuzzing/corpus/frame_codec_no_panic` | `seed-basic.bin`, `seed1` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `result_sequence_state_machine` | `tests/fuzzing/corpus/result_sequence_state_machine` | `seed-basic.bin`, `seed1` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `proto_frame_envelope_decode` | `tests/fuzzing/corpus/proto_frame_envelope_decode` | `seed-basic.bin`, `seed1` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `proto_rpc_execute_request_decode` | `tests/fuzzing/corpus/proto_rpc_execute_request_decode` | `seed-basic.bin`, `seed-valid-reserve-stock-execute.bin` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `proto_rpc_completion_decode` | `tests/fuzzing/corpus/proto_rpc_completion_decode` | `seed-basic.bin`, `seed1` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `proto_invocation_response_sequence_decode` | `tests/fuzzing/corpus/proto_invocation_response_sequence_decode` | `seed-basic.bin`, `seed-valid-metadata-batch-completion.bin` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `srpl_parser_signature_decode` | `tests/fuzzing/corpus/srpl_parser_signature_decode` | `seed-basic.bin`, `seed1` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `srpl_parser_owner_decode` | `tests/fuzzing/corpus/srpl_parser_signature_decode` | `seed-basic.bin`, `seed1` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `storage_wal_record_roundtrip` | `tests/fuzzing/corpus/storage_wal_record_roundtrip` | `seed-basic.bin`, `seed-valid-security-audit-frame.bin`, `seed1` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `wal_record_roundtrip` | `tests/fuzzing/corpus/storage_wal_record_roundtrip` | `seed-basic.bin`, `seed-valid-security-audit-frame.bin`, `seed1` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `btree_node_v1_decode` | `tests/fuzzing/corpus/btree_node_v1_decode` | `seed-basic.bin`, `seed-internal.bin` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `heap_page_v1_decode` | `tests/fuzzing/corpus/heap_page_v1_decode` | `seed-deleted-compacted-parity.bin`, `seed-empty-16k.bin`, `seed-free-offset-crosses-slot-directory.bin`, `seed-header-footer-slot-count-mismatch.bin`, `seed-overlapping-live-tuples.bin`, `seed-single-32k.bin`, `seed-slot-entry-outside-payload-area.bin`, `seed-sparse-deleted-middle-higher.bin`, `seed-two-tuples-16k.bin` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `page_codec_v1_decode` | `tests/fuzzing/corpus/page_codec_v1_decode` | `seed-fixed-row-16k.bin`, `seed-manifest-32k.bin` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `quic_zero_rtt_admission` | `tests/fuzzing/corpus/quic_zero_rtt_admission` | `seed-basic.bin` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `quic_typed_frame_envelope_decode` | `tests/fuzzing/corpus/quic_typed_frame_envelope_decode` | `seed-basic.bin`, `seed-valid-rpc-execute-envelope-frame.bin` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `durable_audit_journal_decode` | `tests/fuzzing/corpus/durable_audit_journal_decode` | `seed-empty-journal.bin`, `seed-hostile-prefix.bin`, `seed-valid-admission-decision.bin`, `seed-valid-generic-audit.bin`, `seed-valid-security-decision.bin` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `rpc_protocol_frame_codec_decode` | `tests/fuzzing/corpus/rpc_protocol_frame_codec_decode` | `seed-basic.bin`, `seed-valid-rpc-execute-frame.bin` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `security_contract_admission_matrix` | `tests/fuzzing/corpus/security_contract_admission_matrix` | `seed-surface-permission-matrix.bin` | `property-assertion-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `manifest_decode` | `tests/fuzzing/corpus/manifest_decode` | `seed-basic.bin` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `segment_index_decode` | `tests/fuzzing/corpus/segment_index_decode` | `seed-basic.bin` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `contract_hash` | `tests/fuzzing/corpus/contract_hash` | `seed-basic.bin` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `definition_batch` | `tests/fuzzing/corpus/definition_batch` | `seed-basic.bin` | `property-assertion-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |
| `structured_object_payload` | `tests/fuzzing/corpus/structured_object_payload` | `seed-basic.bin` | `return-or-error-no-panic` | `OK` | `2026-05-08` | `@AriusII` | `No immediate action` |

## Support modules (non-targets)

- `fuzz_targets/common.rs` is shared by all targets listed with it in `tests/fuzzing/targets.toml`.
- `fuzz_targets/durable_audit_journal_support.rs` is used only by `durable_audit_journal_decode`.

## Procedure

Keep execution rooted in `fuzz/`:

```powershell
Set-Location fuzz
python ../tools/testing/fuzz_registry_check.py --list-targets > fuzz-targets.tsv
```

(or equivalent CI flow), then launch targets from the generated list:

```bash
while IFS=$'\\t' read -r target corpus_dir; do
  cargo +nightly fuzz run "$target" "$corpus_dir" -- -max_total_time=15
done < fuzz-targets.tsv
```

## Validation

After any registry, target, or seed change:

```powershell
python ../tools/testing/fuzz_registry_check.py
python generators/generate_seed_corpus.py --check
```

## Troubleshooting

- `OK` changes to `Missing harness` or `Missing corpus` if a required harness or seed file disappears.
- `security_contract_admission_matrix` and `definition_batch` are the only targets with property `property-assertion-no-panic`.
- `srpl_parser_owner_decode` and `srpl_parser_signature_decode` share one corpus directory.

## References

- `fuzz/README.md`
- `fuzz/VALIDATION_MATRIX.md`
- `tests/fuzzing/README.md`
- `tests/fuzzing/targets.toml`
- `tests/fuzzing/corpus/manifest.toml`
