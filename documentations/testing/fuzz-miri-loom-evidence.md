# Fuzz, Miri, and Loom Evidence

## Purpose

Define the evidence expected for fuzz, Miri, and Loom release gates.

These gates provide deep validation for malformed input, explicit byte formats, undefined-behavior risk, and concurrency interleavings. They do not replace crash/recovery, durable audit, authorization, Procedure contract, or visible-commit evidence.

## Scope

This document applies to release claims that touch:

- WAL and storage byte codecs;
- page, heap, B-Tree, manifest, backup, restore, or recovery inputs;
- SRPL parser and Procedure contract parsing inputs;
- RPC frame, Protobuf envelope, QUIC typed envelope, ResultStream, or admission bytes;
- durable audit journal bytes;
- unsafe, aliasing-sensitive, layout-sensitive, FFI, SIMD, or low-level buffer behavior;
- async, backpressure, shutdown, lock ordering, WAL publication, recovery coordination, or RPC session concurrency.

## Non-goals

- Do not claim that fuzz, Miri, or Loom output is durable database truth.
- Do not use a fuzz target to prove authorization, durable audit, replay, visible commit, TLS identity, QUIC runtime behavior, or HA/DR failover.
- Do not use Miri as a substitute for fuzz, crash/recovery, or protocol compatibility validation.
- Do not use Loom as a substitute for crash/recovery, WAL durability, or security gates.
- Do not create root-level executable harness ownership from this document.

## Prerequisites

Before collecting evidence:

- run from the repository root unless a workflow specifies otherwise;
- record the branch, commit SHA, and `git status --short`;
- record `rustc -Vv`, `cargo -V`, nightly details, and gate-specific tool versions;
- generate or check deterministic seed corpora before fuzzing;
- select targets from `fuzz/targets.toml`;
- name the owning crate and safety property for every Miri or Loom target;
- retain logs, corpora, crash artifacts, model output, and residual-risk notes.

## Procedure

1. Identify the affected surface and owning crate.
2. Select the fuzz targets, Miri targets, or Loom models that match the release claim.
3. Run the preflight command before sustained evidence collection.
4. Run the exact target command and record the duration, toolchain, seed corpus, and artifact path.
5. Classify the result as `Pass`, `Fail`, `Skipped`, or `Partial`.
6. Link the evidence record to the relevant release gate, Step 11 scenario, or owner-suite command.
7. Record any missing target, unsupported operation, unmodeled interleaving, crash artifact, timeout, or flaky result as residual risk.

## Fuzz Gates

Use the deterministic corpus generator before fuzzing:

```powershell
python fuzz/generators/generate_seed_corpus.py --check
```

Use this compile preflight when checking the fuzz crate:

```powershell
cargo check --manifest-path fuzz/Cargo.toml --locked
```

Use this target compile shape when validating a specific fuzz binary:

```powershell
cargo check --manifest-path fuzz/Cargo.toml --bin <target> --locked
```

Use this run shape for each selected target:

```powershell
cargo +nightly fuzz run <target> -- -max_total_time=<seconds>
```

The default 15-second CI run is smoke evidence only. A release promotion must record the actual duration chosen for the release, the target list, the corpus source, any crash artifacts, and the residual risk for surfaces that received only smoke coverage.

## Fuzz Target Evidence Matrix

The current target registry is `fuzz/targets.toml`. The table lists expected evidence per target; it does not claim that any target has passed.

| Target | Owner surface | Required release evidence |
| --- | --- | --- |
| `frame_codec_no_panic` | Generic frame codec malformed-input boundary | Sustained run log, corpus path, panic-free invariant, crash artifacts if any, and owner disposition for malformed frames. |
| `result_sequence_state_machine` | ResultStream sequence state machine | Sustained run log, maximum iterations, sequence invariant, corpus path, and residual risk for executor or durable completion behavior not covered by the target. |
| `proto_frame_envelope_decode` | Generated Protobuf frame envelope decoding | Sustained run log, generated decoder version context, corpus path, and note that runtime transport behavior is not proven. |
| `proto_rpc_execute_request_decode` | Generated RPC execute request decoding | Sustained run log, bounded argument validation evidence, corpus path, and note that catalog resolution, authorization, and executor dispatch are not proven. |
| `proto_rpc_completion_decode` | Generated RPC completion decoding | Sustained run log, completion validation evidence, corpus path, and note that durable completion visibility is not proven. |
| `proto_invocation_response_sequence_decode` | Generated invocation response and ResultStream sequencing | Sustained run log, metadata-batch-completion sequence evidence, corpus path, and residual risk for row payload semantics. |
| `srpl_parser_signature_decode` | SRPL signature parsing boundary | Sustained run log, corpus path, parser rejection behavior, and note that DefinitionBatch publication and catalog recovery are not proven. |
| `srpl_parser_owner_decode` | SRPL owner parsing boundary | Sustained run log, corpus path, parser rejection behavior, and note that Procedure authorization is not proven. |
| `storage_wal_record_roundtrip` | Historical storage-facade WAL record coverage | Sustained run log, corpus path, byte roundtrip invariant, and note that pure WAL owner evidence remains separate. |
| `wal_record_roundtrip` | `andromeda-wal` WAL record codec | Sustained run log, corpus path, byte roundtrip invariant, golden/vector owner links, and crash/recovery gate link before durability promotion. |
| `btree_node_v1_decode` | BTreeNodeV1 decode surface | Sustained run log, corpus path, malformed-node rejection evidence, and note that B-Tree durable mutation recovery is separate. |
| `heap_page_v1_decode` | HeapPageV1 decode surface | Sustained run log, corpus path, slot-directory invariant, and note that manifest publication and replay are separate. |
| `page_codec_v1_decode` | PageCodecV1 decode surface | Sustained run log, corpus path, page header/trailer integrity invariant, and crash/recovery link before page durability promotion. |
| `quic_zero_rtt_admission` | QUIC zero-RTT admission policy | Sustained run log, corpus path, admission matrix invariant, and note that TLS, live QUIC runtime, and durable audit are not proven. |
| `quic_typed_frame_envelope_decode` | QUIC typed frame envelope projection | Sustained run log, corpus path, stream-role and envelope lockstep invariant, and note that Quinn runtime behavior is not proven. |
| `durable_audit_journal_decode` | Durable audit journal byte decoding | Sustained run log, corpus path, journal and anchor bounds, and link to durable audit owner tests before audit readiness claims. |
| `rpc_protocol_frame_codec_decode` | Explicit RPC protocol frame codec | Sustained run log, corpus path, scan/decode/encode invariant, stream-role policy evidence, and note that Procedure routing is separate. |
| `security_contract_admission_matrix` | Runtime-free security admission matrix | Sustained run log, corpus path, fail-closed invariant, permission-family matrix, and note that policy-store correctness and durable audit are separate. |

## Miri Gates

Use broad Miri preflight when it is practical:

```powershell
cargo +nightly miri setup
cargo +nightly miri test --workspace --all-features
```

For release evidence, prefer bounded owner-crate targets when the full workspace is impractical:

```powershell
cargo +nightly miri test -p <owning-crate> <target-name> --all-features
```

For the May 8, 2026 light subset of memory and codec-sensitive crates, print the concrete command list before running anything:

```powershell
python tools/testing/miri_subset.py
```

The current subset is documented in `documentations/testing/miri-subset-2026-05-08.md`. The nightly workflow records this command inventory when `tools/testing/miri_subset.py` is present, but it is an advisory Miri smoke gate for small crates without heavy build scripts, not release-readiness evidence.

Record Miri evidence when the release claim touches:

- unsafe blocks;
- aliasing-sensitive references;
- manual layout or pointer conversion;
- FFI boundaries;
- low-level codecs or byte buffers;
- SIMD fallback boundaries that Miri can exercise;
- concurrency primitives that ordinary tests cannot inspect for undefined behavior.

Miri evidence must include the unsafe or aliasing invariant under test, the command, the nightly version, unsupported operations, and the residual risk. The current nightly workflow uses `continue-on-error` and uploads `nightly-miri-advisory-non-release`; that output is advisory unless a release owner reviews it and the release packet records a blocking pass or an accepted residual risk.

## Loom Gates

A minimal standalone Loom model exists for the WAL durable-before-visible publication rule:

```powershell
cargo test --manifest-path tests/loom/Cargo.toml
```

The nightly deep-validation workflow runs this standalone command as `continue-on-error: true` when `tests/loom/Cargo.toml` is present; otherwise it records the missing harness in `nightly-loom-advisory-non-release`. This model is a bounded proof of the atomic publication ordering only; the nightly artifact is advisory and non-release. Use owner-crate models when a release claim depends on production thread, task, or publication interleavings.

Record Loom evidence when the release claim touches:

- lock ordering;
- cancellation and shutdown;
- backpressure;
- buffer ownership;
- WAL append or publication concurrency;
- recovery coordination;
- RPC or QUIC session state;
- ResultStream state transitions;
- visibility-before-durability prevention.

Loom evidence must name the modeled state, thread or task bounds, explored safety property, feature flags, command, result, and unmodeled behavior. If no Loom model exists for a concurrency-sensitive C5 claim, classify the gate as blocked or out of scope; do not replace it with retries, sleeps, or scheduler-dependent tests.

## Evidence Artifacts

Retain these artifacts for each fuzz, Miri, or Loom record:

| Gate | Required artifacts |
| --- | --- |
| Fuzz | `fuzz/targets.toml`, `fuzz/corpus/manifest.toml`, seed generation log, per-target command log, target duration, `fuzz/corpus/<target>`, `fuzz/artifacts/<target>` when present, toolchain versions, and crash minimization notes. |
| Miri | Miri setup log, command log, nightly toolchain details, owning crate and target, unsafe or aliasing invariant, unsupported-operation notes, and release-owner disposition. |
| Loom | Command log, standalone model path or owning crate and target, feature flags, modeled state, thread/task bounds, safety assertion, explored or reduced state-space note, and residual risk. |

If an artifact path points outside the repository or GitHub Actions artifact storage, record the retention owner and retention period.

## Validation

Validate this document against:

- `fuzz/targets.toml`;
- `fuzz/VALIDATION_MATRIX.md`;
- `.github/workflows/06-nightly-deep-validation.yml`;
- `.github/workflows/07-fuzzing.yml`;
- `tests/miri/README.md`;
- `tests/loom/README.md`;
- `documentations/testing/ci-release-gate-evidence.md`;
- `documentations/testing/step-11-validation-matrix.md`.

This validation is documentation-only. It does not run fuzz, Miri, Loom, or crash/recovery gates.

## Troubleshooting

If a fuzz target fails, retain the crash artifact and classify the release gate as failed until the owning crate fixes the issue or the release owner removes the affected surface from scope.

If a target compiles but has no sustained run, record compile evidence as preflight and keep sustained fuzz evidence open.

If Miri cannot run because a dependency or platform operation is unsupported, create a smaller owner-crate target around the unsafe boundary or record the unsupported operation as residual risk.

If Loom state space is too large, reduce the model to the publication or synchronization boundary that owns the safety property. Do not substitute sleeps, retries, or timing assumptions.

If fuzz, Miri, or Loom evidence conflicts with crash/recovery evidence, treat the crash/recovery or durable visibility failure as the release blocker for C5 claims.

## References

- `fuzz/targets.toml`
- `fuzz/VALIDATION_MATRIX.md`
- `fuzz/README.md`
- `.github/workflows/06-nightly-deep-validation.yml`
- `.github/workflows/07-fuzzing.yml`
- `tests/miri/README.md`
- `tests/loom/README.md`
- `tests/README.md`
- `documentations/testing/ci-release-gate-evidence.md`
- `documentations/testing/release-evidence-template.md`
- `documentations/testing/step-11-validation-matrix.md`
- `docs/codex/rust-critical-quality-gates.md`
- `docs/codex/mission-critical-change-policy.md`
