# Fuzz and Golden Validation Matrix

## Purpose

Document the current readiness matrix for persisted-byte fuzzing and golden/vector validation across the WAL record frame codec, PageCodecV1, HeapPageV1, BTreeNodeV1, RPC frame, Protobuf invocation, typed QUIC envelope, and security admission surfaces.

## Scope

This matrix records a curated set of **high-signal storage-critical and protocol-critical** targets.
It is not the full registry inventory of `tests/fuzzing/targets.toml`; other targets are
listed in that canonical registry and validated by `fuzz_registry_check`.
Executable harness ownership and `cargo-fuzz` execution remain canonical under `fuzz/`.

The matrix is intentionally scoped to evidence-intensive targets where each row is
owned directly by a canonical crate owner and has explicit golden/property/fuzz
follow-up instructions.

## Non-goals

- Do not redefine the binary formats.
- Do not change fuzz harness behavior.
- Do not add, remove, or regenerate seed files outside the named Lot 4.6C deterministic corpus update.
- Do not replace crash/recovery validation with fuzz or golden evidence.
- Do not move page, manifest, or segment ownership in Lot 4.6C.
- Do not treat Lot 5 fuzz coverage as authorization, admission, transport, or audit durability evidence.
- Do not introduce JSON, gRPC, ad hoc SQL, native Rust struct wire images, or runtime QUIC/TLS behavior through fuzz harnesses.

## Lot 4.6C Page Durable-Format Evidence Preflight

Risk class: mission-critical storage evidence, additive test and deterministic-corpus evidence.

Lot 4.6C closes page-format evidence gaps after the FileWal ownership move. It does not move runtime code, public imports, persistent bytes, or crate ownership for pages, manifests, or segments.

Keep these evidence boundaries separate:

- `PageCodecV1` evidence proves the explicit codec for a canonical page byte image: fixed header, payload, and trailer with little-endian fields and integrity values.
- `DiskPageStore` persisted-layout evidence proves file-backed store integration for page images, including overlays and readback through the store. It is integration evidence, not proof that `PageCodecV1` has complete full-page golden-vector coverage.
- Heap and B+Tree vectors prove surface-specific payload layouts that may be carried by a page image. They do not prove manifest publication, cold-segment publication, startup replay, or visible-commit durability.
- Golden/vector and fuzz evidence must stay subordinate to crash/recovery validation before any page, manifest, segment, or durability promotion.

## Lot 5 RPC, QUIC, and Security Evidence Preflight

Risk class: mission-critical protocol and security evidence, additive fuzz and deterministic-corpus coverage.

Lot 5 adds protocol and admission fuzz surfaces without moving runtime ownership or changing on-wire formats. The targets exercise generated Protobuf decoders, explicit RPC frame codecs, typed QUIC envelope validation, and runtime-free security admission contracts only.

Keep these evidence boundaries separate:

- `andromeda-rpc-protocol` owns explicit frame bytes, fixed header layout, CRC validation, and bounded frame scanning.
- `andromeda-proto` owns generated Protobuf message decoding and generated validation for invocation requests and ResultStream responses.
- `andromeda-quic` owns typed envelope lockstep between decoded RPC frames and generated Protobuf envelopes, but the fuzz target does not enable the Quinn runtime.
- `andromeda-security-contract` owns stable admission codes, surface boundaries, and permission-family matrices. It does not authorize requests or emit durable audit evidence.
- Fuzz evidence is malformed-input and invariant evidence only. It does not prove admission durability, WAL persistence, replay, or visible commit.
- Smoke fuzz runs and harness compile checks are preflight signals only; they do not prove release/C5 readiness.

## Validation Matrix

| Surface | Fuzz target | Current seeds | Golden/vector coverage | Missing readiness item |
| --- | --- | --- | --- | --- |
| WAL record frame codec and physical FileWal (`andromeda-wal` owner) | `wal_record_roundtrip` in `fuzz_targets/wal_record_roundtrip.rs` targets the owning crate directly. `storage_wal_record_roundtrip` remains as historical storage-facade coverage. | `seed-basic.bin`, `seed-valid-security-audit-frame.bin`, and `seed1` in `tests/fuzzing/corpus/storage_wal_record_roundtrip`; both WAL targets list the shared corpus in `tests/fuzzing/corpus/manifest.toml` and use deterministic bytes from `fuzz/generators/generate_seed_corpus.py`. | Owner evidence lives in `crates/andromeda-wal/tests/wal_codec_contract.rs` for golden/corrupt frame vectors and scan-stop classification, plus `crates/andromeda-wal/tests/property_wal_roundtrip.rs` for bounded production-codec properties. Lot 4.5 adds `crates/andromeda-wal/tests/file_wal_contract.rs` as physical FileWal owner evidence for file header, open/append, scan, and byte-contract invariants without changing the disk format. `crates/andromeda-storage/tests/api_compat_reexports.rs`, `wal_ownership_invariants.rs`, `wal_scan_recovery_contract.rs`, and `file_wal_recovery_contract.rs` remain storage API, ownership-boundary, and recovery integration evidence. | Attach sustained `wal_record_roundtrip` fuzz run evidence before promoting the pure frame codec. Physical FileWal promotion requires `andromeda-wal` owner contract evidence and storage API/ownership/recovery integration evidence. Owner fuzz/golden/FileWal coverage does not prove startup recovery reports, replay planning, manifest publication, page integration, or durable visibility by itself. |
| PageCodecV1 | `page_codec_v1_decode` in `fuzz_targets/page_codec_v1_decode.rs` | `seed-fixed-row-16k.bin`, `seed-manifest-32k.bin` in `tests/fuzzing/corpus/page_codec_v1_decode`; both listed in `tests/fuzzing/corpus/manifest.toml` and generated by `fuzz/generators/generate_seed_corpus.py`. | `crates/andromeda-storage/src/page_codec_v1/tests.rs` covers header/trailer roundtrip, payload CRC validation, malformed rejection, length gates, min/max fields, golden header prefix bytes, corpus-sized encode/decode pages, and Lot 4.6C full-page 16 KiB and 32 KiB golden images. The full-page vectors pin exact image length, payload bytes, trailer bytes, payload CRC64, payload hash, torn-write guard, image SHA-256, and decode/re-encode stability. `crates/andromeda-storage/tests/property_page_codec_v1.rs` covers production `PageCodecV1` encode/decode stability and basic corruption rejection. This is codec evidence only; `DiskPageStore` persisted-layout readback is separate integration evidence. | Record sustained `page_codec_v1_decode` fuzz run evidence before promoting the page codec beyond Lot 4.6C evidence. Keep this evidence independent from `DiskPageStore` overlay/persistence evidence and subordinate to crash/recovery validation for any page, manifest, segment, or durability promotion. |
| HeapPageV1 | `heap_page_v1_decode` in `fuzz_targets/heap_page_v1_decode.rs` | `seed-empty-16k.bin`, regenerated durable-offset `seed-single-32k.bin` and `seed-two-tuples-16k.bin`, plus Lot 4.6C named seeds `seed-deleted-compacted-parity.bin`, `seed-sparse-deleted-middle-live-higher.bin`, `seed-header-footer-slot-count-mismatch.bin`, `seed-free-offset-crosses-slot-directory.bin`, `seed-slot-entry-outside-payload-area.bin`, and `seed-overlapping-live-tuples.bin` in `tests/fuzzing/corpus/heap_page_v1_decode`; all listed in `tests/fuzzing/corpus/manifest.toml` and generated by `fuzz/generators/generate_seed_corpus.py`. | `crates/andromeda-storage/tests/heap_golden_vectors.rs` covers empty, single tuple, multiple tuple, deleted tuple, compacted page, max/min tuple, mutation determinism, replay idempotency, slot-entry roundtrip, and heap-page readback. `crates/andromeda-storage/tests/heap_page_contract/durable_layout.rs` covers durable layout constants, 16 KiB and 32 KiB slot-directory byte placements, `PageCodecV1` header overlay, and `DiskPageStore` persistence as integration evidence. Lot 4.6C adds named slot-directory vectors in `crates/andromeda-storage/tests/heap_page_contract/slot_directory.rs` for deleted/compacted parity, header/footer mismatch, free-offset crossing, out-of-area slot entries, overlapping live tuples, and sparse deleted-middle/live-higher directories. | Record sustained `heap_page_v1_decode` run evidence against the deterministic corpus before promoting HeapPageV1 beyond Lot 4.6C evidence. Heap vectors still do not prove manifest publication, startup replay, page flush persistence, or visible-commit durability by themselves. |
| BTreeNodeV1 | `btree_node_v1_decode` in `fuzz_targets/btree_node_v1_decode.rs` | `seed-basic.bin`, `seed-internal.bin` in `tests/fuzzing/corpus/btree_node_v1_decode`; both listed in `tests/fuzzing/corpus/manifest.toml` and generated by `fuzz/generators/generate_seed_corpus.py`. | `crates/andromeda-storage/tests/btree_node_golden_vectors.rs` groups stable byte-format coverage across `golden_header`, `golden_full_nodes`, `roundtrip`, `decode_header_gates`, `decode_body_gates`, and `persistence_validation`. The suite covers empty leaf golden header bytes, Lot 4.6C non-empty leaf and non-empty internal full-node golden vectors, leaf/internal roundtrips, page-id checks, reserved/header corruption rejection, body truncation, duplicate keys, zero child pointers, and persistence validation gates. | Record sustained `btree_node_v1_decode` fuzz run evidence before promoting BTreeNodeV1 beyond Lot 4.6C evidence. Add a non-empty leaf corpus seed if future promotion requires corpus symmetry with the existing non-empty internal seed. |
| Generated RPC execute request | `proto_rpc_execute_request_decode` in `fuzz_targets/proto_rpc_execute_request_decode.rs` | `seed-basic.bin`, `seed-valid-reserve-stock-execute.bin` in `tests/fuzzing/corpus/proto_rpc_execute_request_decode`; both listed in `tests/fuzzing/corpus/manifest.toml` and generated by `fuzz/generators/generate_seed_corpus.py`. | Generated validation evidence lives in `crates/andromeda-proto/src/generated_validation/runtime_projection/execute_args.rs` and `crates/andromeda-proto/tests/invocation_boundary_contract/*`. The fuzz target decodes generated `RpcExecuteRequest`, runs `validate_generated_rpc_execute_request`, and keeps argument and budget inspection bounded. | Record sustained fuzz run evidence before treating generated execute-request malformed-input coverage as complete. This does not prove Procedure authorization, catalog resolution, security admission, or executor dispatch. |
| Generated invocation response ResultStream sequence | `proto_invocation_response_sequence_decode` in `fuzz_targets/proto_invocation_response_sequence_decode.rs` | `seed-basic.bin`, `seed-valid-metadata-batch-completion.bin` in `tests/fuzzing/corpus/proto_invocation_response_sequence_decode`; the valid seed uses a bounded length-prefixed sequence of generated `InvocationResponse` messages. | Sequence evidence lives in `crates/andromeda-proto/src/generated_validation/runtime_projection/frame_result_metadata/invocation_response_sequence.rs` and `crates/andromeda-proto/tests/invocation_boundary_contract/resultstream.rs`. The fuzz target validates individual responses and complete metadata-batch-completion sequences. | Record sustained fuzz run evidence before promoting generated ResultStream malformed-input coverage. This does not prove row payload semantics, executor output correctness, durable completion, or WAL visibility. |
| QUIC typed frame envelope | `quic_typed_frame_envelope_decode` in `fuzz_targets/quic_typed_frame_envelope_decode.rs` | `seed-basic.bin`, `seed-valid-rpc-execute-envelope-frame.bin` in `tests/fuzzing/corpus/quic_typed_frame_envelope_decode`; the valid seed is an explicit RPC frame carrying a generated `FrameEnvelope`. | Typed envelope evidence lives in `crates/andromeda-quic/src/typed_envelope.rs` and `crates/andromeda-quic/tests/protobuf_projection_contract.rs`. The fuzz target decodes explicit frames, validates stream-role policy, validates typed envelope context lockstep, and exercises bounded typed ResultStream checks without enabling `runtime-quinn`. | Record sustained fuzz run evidence before promoting typed envelope malformed-input coverage. This does not prove Quinn runtime behavior, TLS, retry, admission durability, or HA/DR stream correctness. |
| RPC protocol frame codec | `rpc_protocol_frame_codec_decode` in `fuzz_targets/rpc_protocol_frame_codec_decode.rs` | `seed-basic.bin`, `seed-valid-rpc-execute-frame.bin` in `tests/fuzzing/corpus/rpc_protocol_frame_codec_decode`; both listed in `tests/fuzzing/corpus/manifest.toml` and generated by `fuzz/generators/generate_seed_corpus.py`. | Frame codec evidence lives in `crates/andromeda-rpc-protocol/src/frame_codec.rs` and `crates/andromeda-rpc-protocol/tests/frame_wire_contract.rs`. The fuzz target uses `andromeda-rpc-protocol` directly, exercises scan/decode/encode roundtrip, and validates stream role policy through the protocol owner. | Record sustained fuzz run evidence before promoting frame-codec malformed-input coverage. This does not prove QUIC network behavior or Procedure routing. |
| Security admission contract matrix | `security_contract_admission_matrix` in `fuzz_targets/security_contract_admission_matrix.rs` | `seed-surface-permission-matrix.bin` in `tests/fuzzing/corpus/security_contract_admission_matrix`; listed in `tests/fuzzing/corpus/manifest.toml` and generated by `fuzz/generators/generate_seed_corpus.py`. | Runtime-free security admission evidence lives in `crates/andromeda-security-contract/src/admission.rs`. The fuzz target asserts stable code roundtrips, missing-evidence fail-closed behavior, and surface-to-permission-family matrix consistency. | Record sustained fuzz run evidence before promoting admission-code malformed-input coverage. This does not authorize requests, emit durable audit records, validate certificate identity, or prove policy-store correctness. |

## Validation

Documentation claims in this file should stay traceable to:

- `tests/fuzzing/targets.toml`
- `tests/fuzzing/corpus/manifest.toml`
- `fuzz/generators/generate_seed_corpus.py`
- `fuzz_targets/*.rs`
- `crates/andromeda-rpc-protocol/tests/frame_wire_contract.rs` for explicit RPC frame codec evidence
- `crates/andromeda-proto/tests/invocation_boundary_contract/*` for generated invocation and ResultStream validation evidence
- `crates/andromeda-quic/tests/protobuf_projection_contract.rs` for typed envelope and ResultStream projection evidence
- `crates/andromeda-security-contract/src/admission.rs` for runtime-free admission contract evidence
- `crates/andromeda-wal/tests/*` for pure WAL and physical FileWal owner evidence
- `crates/andromeda-storage/tests/api_compat_reexports.rs`, `wal_ownership_invariants.rs`, `wal_scan_recovery_contract.rs`, and `file_wal_recovery_contract.rs` for storage compatibility, ownership-boundary, and recovery integration evidence
- `crates/andromeda-storage/tests/*` for page, heap, B+Tree, manifest, and broader recovery integration evidence
- `crates/andromeda-storage/src/page_codec_v1/tests.rs`

For Lot 4.6C page durable-format evidence preflight, run:

```powershell
python fuzz/generators/generate_seed_corpus.py --check
cargo test -p andromeda-storage page_codec_v1 -- --nocapture
cargo test -p andromeda-storage --test property_page_codec_v1 -- --nocapture
cargo test -p andromeda-storage --test heap_golden_vectors -- --nocapture
cargo test -p andromeda-storage --test heap_page_contract -- --nocapture
cargo test -p andromeda-storage --test btree_node_golden_vectors -- --nocapture
cargo check --manifest-path fuzz/Cargo.toml --bin page_codec_v1_decode --locked
cargo check --manifest-path fuzz/Cargo.toml --bin heap_page_v1_decode --locked
cargo check --manifest-path fuzz/Cargo.toml --bin btree_node_v1_decode --locked
```

For Lot 5 RPC, QUIC, Protobuf, and security admission evidence preflight, run:

```powershell
python fuzz/generators/generate_seed_corpus.py --check
cargo test -p andromeda-rpc-protocol --test frame_wire_contract -- --nocapture
cargo test -p andromeda-proto --test invocation_boundary_contract -- --nocapture
cargo test -p andromeda-quic --test codec_contract -- --nocapture
cargo test -p andromeda-quic --test protobuf_projection_contract -- --nocapture
cargo test -p andromeda-security-contract --lib -- --nocapture
cargo check --manifest-path fuzz/Cargo.toml --bin proto_rpc_execute_request_decode --locked
cargo check --manifest-path fuzz/Cargo.toml --bin proto_invocation_response_sequence_decode --locked
cargo check --manifest-path fuzz/Cargo.toml --bin quic_typed_frame_envelope_decode --locked
cargo check --manifest-path fuzz/Cargo.toml --bin rpc_protocol_frame_codec_decode --locked
cargo check --manifest-path fuzz/Cargo.toml --bin security_contract_admission_matrix --locked
```

The following FileWal evidence commands remain the gate for the existing Lot 4.5 row and are not part of the Lot 4.6C page ownership scope:

```powershell
cargo test -p andromeda-wal --tests
cargo test -p andromeda-wal --test file_wal_contract -- --nocapture
cargo test -p andromeda-storage --test api_compat_reexports -- --nocapture
cargo test -p andromeda-storage --test wal_ownership_invariants -- --nocapture
cargo test -p andromeda-storage --test file_wal_recovery_contract -- --nocapture
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo check --manifest-path fuzz/Cargo.toml --bin wal_record_roundtrip --locked
```

For FileWal physical ownership, continue to use `andromeda-wal` owner contract evidence and storage API/ownership/recovery integration evidence. For readiness promotion, add sustained fuzz run evidence for the specific owner surface being promoted. For recovery, page, manifest, segment, or visibility promotion, add targeted storage tests and crash/recovery evidence for the integration surface as well.
Treat sustained fuzz evidence as still required unless an explicit release checkpoint records duration, arguments, crash/OOM/timeout counters, and artifact disposition.

## Troubleshooting

If a target, corpus directory, or seed list drifts, treat `tests/fuzzing/targets.toml` and `tests/fuzzing/corpus/manifest.toml` as the authoritative registry pair and update this matrix only after the generator check passes.

If a golden/vector suite moves, keep this file pointed at the stable owning test module rather than duplicating the vector bytes here.

## References

- `fuzz/README.md`
- `tests/fuzzing/targets.toml`
- `tests/fuzzing/corpus/manifest.toml`
- `fuzz/generators/generate_seed_corpus.py`
- `fuzz_targets/wal_record_roundtrip.rs`
- `fuzz_targets/storage_wal_record_roundtrip.rs`
- `fuzz_targets/page_codec_v1_decode.rs`
- `fuzz_targets/heap_page_v1_decode.rs`
- `fuzz_targets/btree_node_v1_decode.rs`
- `fuzz_targets/proto_rpc_execute_request_decode.rs`
- `fuzz_targets/proto_invocation_response_sequence_decode.rs`
- `fuzz_targets/quic_typed_frame_envelope_decode.rs`
- `fuzz_targets/rpc_protocol_frame_codec_decode.rs`
- `fuzz_targets/security_contract_admission_matrix.rs`
