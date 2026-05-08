# Roadmap 11.07 Fuzz Target Gaps and Sustained-Run Evidence

## Purpose

Document the Roadmap 11.07 fuzz target gaps and the sustained-run evidence plan for Andromeda persisted-byte, contract, catalog, and protocol boundaries.

This document is a planning artifact. It does not add targets, change fuzz behavior, change seed corpus contents, or promote any fuzz surface to release evidence by itself.

## Scope

This roadmap covers the following fuzz evidence surfaces:

- Missing target candidates: `manifest_decode`, `segment_index_decode`, `contract_hash`, `definition_batch`, and `structured_object_payload`.
- Current Lot 5 harnesses already registered in `tests/fuzzing/targets.toml`.
- Corpus policy for deterministic committed seeds and generated seed manifests.
- No-panic scope for malformed input handling.
- Evidence boundaries for what fuzzing does not prove.

Affected engines and planes:

- Storage engine: page, heap, B+Tree, manifest, segment index, WAL-facing storage records, and backup manifest decode boundaries.
- Catalog and Procedure contract plane: `ContractHash`, DefinitionBatch planning, source evidence, dependency graph evidence, and contract compatibility.
- StructuredObject plane: header validation, descriptor hashing, row-count policy, and payload length bounds.
- RPC and protocol plane: generated Protobuf decoders, explicit RPC frames, typed QUIC envelope checks, ResultStream sequence checks, and runtime-free security admission contracts.

## Non-goals

- Do not edit `fuzz/Cargo.toml`, `tests/fuzzing/targets.toml`, any file under `fuzz/fuzz_targets/`, or any corpus file as part of this roadmap document.
- Do not define new persistent formats here.
- Do not treat fuzz evidence as crash/recovery evidence.
- Do not treat fuzz evidence as authorization, admission durability, WAL durability, manifest publication, startup replay, visible commit, TLS, Quinn runtime, or HA/DR evidence.
- Do not introduce ad hoc SQL, gRPC, JSON runtime protocols, dynamic SRPL surfaces, native Rust struct wire images, or GPU work in a critical path.
- Do not promote any surface without owner tests, deterministic vectors where applicable, and sustained-run evidence attached to the relevant release checkpoint.

## Prerequisites

Before adding any Roadmap 11.07 target, confirm that:

- The production boundary has an explicit codec, parser, validator, or typed adapter that can reject malformed input without panicking.
- The harness can stay bounded by input size, iteration count, allocation limits, and recursion limits.
- The target has a deterministic seed plan that can be represented in `tests/fuzzing/corpus/manifest.toml`.
- The target owner is clear. Storage-owned targets must not be routed through compatibility facades when an owning crate API exists.
- The no-panic property is meaningful for the target boundary and does not require swallowing invariant violations that production code must reject.
- The target can be run without external services, network runtimes, nondeterministic clocks, GPU jobs, or durable state mutation outside the fuzz artifact directory.

## Current Lot 5 Harnesses

Lot 5 already has registered harnesses for RPC, Protobuf, QUIC projection, ResultStream, durable audit, and security admission evidence. These targets should receive sustained-run evidence before any release claim relies on them.

| Target | Surface | Current evidence role | Sustained-run requirement |
| --- | --- | --- | --- |
| `proto_frame_envelope_decode` | Generated `FrameEnvelope` decode and validation | Malformed generated-envelope input coverage | Attach duration, commit, corpus hash, sanitizer mode, crash count, and artifact status before promotion. |
| `proto_rpc_execute_request_decode` | Generated `RpcExecuteRequest` decode and generated validation | Procedure invocation request malformed-input coverage | Prove bounded decode and validation do not panic; do not claim Procedure authorization or executor dispatch. |
| `proto_rpc_completion_decode` | Generated RPC completion decode | Completion message malformed-input coverage | Prove decode and validation are bounded; do not claim durable completion or ResultStream correctness. |
| `proto_invocation_response_sequence_decode` | Generated `InvocationResponse` sequence and ResultStream metadata-batch-completion ordering | Sequence malformed-input and state validation coverage | Include valid metadata-batch-completion seeds and long-run evidence; do not claim row payload semantics. |
| `rpc_protocol_frame_codec_decode` | Explicit RPC frame codec | Frame scan, decode, encode, and stream-role policy coverage | Run against deterministic frame seeds and mutated frames; do not claim QUIC network behavior. |
| `quic_typed_frame_envelope_decode` | Typed QUIC envelope projection over decoded RPC frames and generated Protobuf envelopes | Runtime-free envelope lockstep coverage | Keep `runtime-quinn` out of scope; do not claim TLS, retry, network scheduling, or HA/DR correctness. |
| `quic_zero_rtt_admission` | QUIC zero-RTT admission policy inputs | Runtime-free replay and admission decision coverage | Prove policy classification is bounded and typed; do not claim actual session resumption behavior. |
| `security_contract_admission_matrix` | Security admission codes and surface-to-permission-family matrix | Runtime-free fail-closed and code stability coverage | Prove stable code handling and matrix consistency; do not claim request authorization or durable audit emission. |
| `durable_audit_journal_decode` | Durable audit journal decode support | Audit journal malformed-input coverage | Prove decode rejection and bounded scanning; do not claim retention policy, fsync, or audit sink durability. |
| `result_sequence_state_machine` | Result sequence ordering model | State-machine ordering and no-panic coverage | Keep iteration limits explicit; do not claim executor output correctness. |
| `frame_codec_no_panic` | Legacy or broad frame no-panic coverage | General malformed frame regression coverage | Keep as compatibility coverage unless owner-specific frame targets supersede it. |

## Roadmap 11.07 Gaps

The following gaps should be closed as additive fuzz targets after owner APIs and deterministic seeds are ready.

| Gap | Candidate target | Owner surface | Primary property | Seed policy | Promotion evidence |
| --- | --- | --- | --- | --- | --- |
| Manifest decode | `manifest_decode` | Storage manifest or backup manifest explicit decode boundary, depending on the owner selected by the implementation task | Arbitrary bytes are rejected or decoded without panic, with version, length, checksum, and cross-field gates preserved | Include minimal empty/invalid bytes, valid current-version manifest bytes, bad version, truncated body, checksum mismatch, and inconsistent root pointer or checkpoint references when the owner format exposes them | Owner unit/golden tests, corpus check, `cargo check --manifest-path fuzz/Cargo.toml --bin manifest_decode --locked`, and sustained fuzz run evidence |
| Segment index decode | `segment_index_decode` | Cold segment or storage segment index explicit decode boundary | Segment index bytes reject out-of-order entries, overlapping ranges, impossible page or extent references, invalid checksums, and length overflow without panic | Include empty index, single-entry valid index, multi-entry valid index, overlap, descending key range, truncated footer, and corrupt checksum seeds | Owner golden vectors, storage integration tests for segment publication boundaries, corpus check, target check, and sustained fuzz run evidence |
| Contract hash | `contract_hash` | `andromeda-types`, `andromeda-contract`, and structured descriptor hashing boundaries | Hash constructors, canonical hash derivation, zero sentinel rules, display/parse-adjacent handling, and shape-hash canonicalization remain bounded and deterministic | Include exact 32-byte hashes, 31-byte and 33-byte inputs, all-zero sentinel, descriptor order variants, Procedure contract variants, and StructuredObject descriptor variants | Existing `ContractHash` and contract-hash golden tests plus sustained fuzz evidence; do not claim catalog compatibility or Procedure authorization |
| DefinitionBatch | `definition_batch` | Catalog DefinitionBatch planning and SRPL DefinitionBatch dry-run bridge | Bounded generated DefinitionBatch-like inputs reject invalid counts, missing dependencies, duplicate objects, stale base versions, bad source evidence, and dependency graph inconsistencies without panic | Prefer grammar-aware or structured byte generation over unconstrained native struct images; include one valid inventory-domain-style batch, empty batch, duplicate Procedure, unresolved dependency, stale version, and excessive source count seeds | Catalog owner tests, SRPL dry-run compatibility tests, WAL catalog record tests, corpus check, target check, and sustained fuzz evidence; crash/recovery remains separate |
| StructuredObject payload | `structured_object_payload` | `andromeda-structured-object` header validation and any explicit payload decoder once present | Header, descriptor hash, row-count policy, payload length, layout enum, field ordinals, and zero/nonzero hash rules reject malformed inputs without panic | Include valid header with empty payload where allowed, valid exact-row payload, zero contract hash, zero descriptor hash, duplicate field names, sparse ordinals, declared positive rows with empty payload, empty rows with non-empty payload, and over-bound payload | StructuredObject unit tests, descriptor hash vectors, corpus check, target check, and sustained fuzz evidence; do not claim executor row semantics or wire payload compatibility until a payload codec exists |

## Procedure

Use this sequence for each Roadmap 11.07 fuzz gap.

1. Confirm the owner API and evidence boundary.
2. Add or update owner tests before adding a fuzz target.
3. Define bounded input interpretation for the harness.
4. Add deterministic seeds through `fuzz/generators/generate_seed_corpus.py`.
5. Register the target in `tests/fuzzing/targets.toml`, `fuzz/Cargo.toml`, and `tests/fuzzing/corpus/manifest.toml` in the same implementation change.
6. Run the corpus generator check.
7. Run `cargo check --manifest-path fuzz/Cargo.toml --bin <target> --locked`.
8. Run a short local smoke fuzz job.
9. Run a sustained fuzz job for release evidence.
10. Store only the evidence summary, not generated artifacts, unless the release process names an artifact retention location.

## Corpus Policy

Committed corpus seeds must be deterministic, small, and explainable. Use them to enter important format states quickly, not to replace mutation coverage.

Required corpus rules:

- Every target must have an entry in `tests/fuzzing/corpus/manifest.toml`.
- Every seed must be generated or checked by `fuzz/generators/generate_seed_corpus.py`.
- Valid seeds should pin current canonical formats where those formats already exist.
- Invalid seeds should target specific rejection gates: bad version, bad length, truncated body, bad checksum, zero sentinel misuse, duplicate identity, dependency mismatch, and declared-count mismatch.
- Seeds must not depend on host paths, wall-clock time, randomness, network services, GPU output, or files under `target/`.
- Corpus additions should prefer named binary files over opaque `seed1` files when the seed has a durable evidence role.

## No-Panic Scope

For Roadmap 11.07, no-panic means the fuzz boundary handles arbitrary byte input by returning, rejecting, or reporting a typed error without panicking.

No-panic scope includes:

- Decode, parse, validation, and canonicalization entry points used by the harness.
- Length, count, version, checksum, hash, and enum discriminant gates.
- Bounded allocation and iteration behavior.
- Rejection of malformed data before large allocation or unbounded recursion.

No-panic scope does not mean:

- The input is valid.
- The decoded object is durable.
- The decoded object can be published.
- The operation is authorized.
- The operation has been written to WAL.
- The system can recover from a crash at that boundary.
- The executor, QUIC runtime, catalog publisher, or HA/DR subsystem has accepted the object.

## Validation

For this documentation-only roadmap update, validate with:

```powershell
git diff --check -- fuzz/ROADMAP_FUZZ_GAPS_2026.md
```

For implementation changes that add Roadmap 11.07 fuzz targets, validate with the applicable owner tests plus:

```powershell
python fuzz/generators/generate_seed_corpus.py --check
cargo check --manifest-path fuzz/Cargo.toml --bin <target> --locked
cargo fuzz run <target> -- -max_total_time=<seconds>
```

Record sustained-run evidence with at least:

- Repository commit.
- Target name.
- Target source path.
- Corpus manifest hash or seed list.
- Run duration and libFuzzer arguments.
- Sanitizer and build profile.
- Host platform.
- Crash count.
- Timeout count.
- OOM count.
- Artifact path or explicit "no artifact produced" note.
- Owner tests run before the sustained job.

## Troubleshooting

If the target name, corpus directory, or seed list drifts, reconcile `tests/fuzzing/targets.toml`, `fuzz/Cargo.toml`, and `tests/fuzzing/corpus/manifest.toml` before accepting evidence.

If a harness requires production-only runtime state, reduce the target to the explicit codec, validator, or typed adapter boundary. Fuzz jobs must not require live network sessions, durable cluster state, or external services.

If fuzzing finds a panic, preserve the minimized reproducer outside committed corpus until the fix is understood. Add the reproducer to the deterministic corpus only after it is stable, non-sensitive, and tied to a named regression gate.

If a surface has no explicit codec or typed validator, do not add a byte-level fuzz target yet. Add owner tests and a bounded API first.

## References

- `AGENTS.md`
- `fuzz/README.md`
- `fuzz/VALIDATION_MATRIX.md`
- `tests/fuzzing/targets.toml`
- `tests/fuzzing/corpus/manifest.toml`
- `fuzz/generators/generate_seed_corpus.py`
- `crates/README.md`
- `crates/andromeda-types/src/ids.rs`
- `crates/andromeda-contract/tests/contract_hash_golden.rs`
- `crates/andromeda-structured-object/src/lib.rs`
- `crates/andromeda-catalog/src/batch/definition.rs`
- `crates/andromeda-srpl/src/definition_batch_bridge/dry_run.rs`
- `crates/andromeda-storage/src/manifest.rs`
- `crates/andromeda-storage/src/manifest/format.rs`
