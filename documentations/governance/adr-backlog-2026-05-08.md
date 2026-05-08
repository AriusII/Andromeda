# ADR Backlog 2026-05-08

## Purpose

Track the Architecture Decision Record backlog and closure status raised by
roadmap step 12 for the Andromeda implementation wave.

This backlog identifies decisions that needed ADR coverage before the
repository could treat the related runtime, storage, hardware, or topology
behavior as governed by accepted documentation. Accepted ADRs still do not make
the related implementation release-ready; runtime claims require retained owner
tests, crash/recovery evidence, and release-gate disposition.

## Scope

This document covers six Step 12 ADR candidates and their current disposition:

- Unsafe Rust policy, accepted as ADR-0013.
- Binary format and endian policy, accepted as ADR-0014.
- WAL, commit visibility, and `DurableCommitEvidence`, accepted as ADR-0015.
- GPU exclusion from critical paths, accepted as ADR-0016.
- Rust toolchain and MSRV policy, accepted as ADR-0017.
- Macro-engine mapping and crate topology, accepted as ADR-0018.

The backlog records priority, proposed owner, disposition, source evidence,
acceptance criteria, and residual risk for each candidate. For accepted items,
it records the accepted ADR that owns the decision and the implementation
evidence still required before release claims.

## Non-goals

- Do not edit existing ADR or DEC files.
- Do not allocate new ADR numbers from this backlog alone.
- Do not change Rust code, Cargo manifests, tests, hooks, or CI.
- Do not revise accepted doctrine by implication.
- Do not claim that partial runtime scaffolds are production-ready.

## Prerequisites

Reviewers should read the following active doctrine before drafting any ADR
from this backlog:

- `AGENTS.md` for non-negotiable invariants, Rust baseline, unsafe policy, and
  explicit binary codec expectations.
- `docs/adr/ADR-0005-rust-2024-baseline.md` for the accepted Rust 2024
  baseline.
- `docs/adr/ADR-0011-workspace-crate-boundaries.md` for dependency rings and
  current macro-engine crate ownership.
- `docs/adr/ADR-0013-unsafe-rust-policy.md` through
  `docs/adr/ADR-0018-engine-crate-mapping-policy.md` for Step 12 accepted ADRs.
- `documentations/ROADMAP_IMPLEMENTATION_2026.md` for the current priority
  roadmap and open decisions.
- `documentations/WORKER_EXECUTION_MATRIX_2026.md` for worker outcomes,
  release blockers, and documentation rules.
- `documentations/CURRENT_STATE.md` for current implementation status and
  residual runtime work.
- `documentations/governance/decisions/DEC-026-release-gates-and-deferral-policy.md`
  for release gate expectations.
- `documentations/governance/decisions/DEC-032-storage-format-gate.md` for
  storage format promotion gates.
- `documentations/governance/step-12-documentation-closure-2026-05-08.md` for
  the Step 12 documentation closure checklist.

## Relevant invariants

These invariants must remain true for every ADR in this backlog:

- Application behavior remains exposed through typed, cataloged Procedures.
- Application-facing ad hoc SQL remains prohibited.
- Visible mutation requires durable WAL evidence.
- RAM, temp storage, GPU output, benchmark output, and audit records are not
  database truth.
- Reconstructible database truth remains the last valid cold snapshot plus
  durable WAL from that snapshot.
- GPU work must not participate in commit, WAL, rollback, recovery, MVCC
  visibility, catalog publication, or security-critical paths.
- Persistent and network bytes require explicit codecs, format identity, and
  rejection behavior. Rust native struct layout is not a storage or network
  contract.
- Optimization and adaptive behavior must be bounded, observable, explainable,
  versioned, and disableable.

## Procedure

Use this backlog as follows:

1. Locate the candidate that governs the implementation work under review.
2. Open the accepted ADR named in the disposition column.
3. Cite the ADR, source evidence, acceptance criteria, and validation commands
   from the accepted record.
4. Treat this backlog as traceability, not approval for runtime behavior.
5. Update the ADR or DEC index only in a separate owned documentation task.
6. Keep this backlog until the release owner either archives the Step 12 trace
   or moves residual implementation evidence requirements into a more specific
   roadmap document.

## Backlog

| ID | Candidate ADR | Priority | Proposed owner | Disposition | Source evidence | Acceptance criteria |
|---|---|---:|---|---|---|---|
| ADR-BL-001 | Unsafe Rust policy | P0 | Rust safety and release governance owners | Accepted in `docs/adr/ADR-0013-unsafe-rust-policy.md` | `AGENTS.md` requires `unsafe` to stay private, documented, tested, and reviewed. `DEC-026` makes unsafe drift a doctrine no-go scan. `CURRENT_STATE.md` records broad runtime work but does not make unsafe policy release-specific. | ADR-0013 defines allowed and forbidden unsafe locations, required comments, review gates, Miri/fuzz/property expectations, panic/UB escalation, and release-blocking scan behavior. |
| ADR-BL-002 | Binary format and endian policy | P0 | Binary format, storage, WAL, and protocol owners | Accepted in `docs/adr/ADR-0014-binary-format-endian-policy.md` | `AGENTS.md` requires explicit binary codecs and little-endian canonical serialization. `DEC-032` requires storage format identity, golden vectors, unknown-format rejection, and recovery validation before redo. `FrameHeader_RPC_v0.md` records a named network-frame byte-order exception. | ADR-0014 defines default endian policy, persistent versus network-frame exceptions, version fields, checksums, length bounds, golden vectors, cross-architecture tests, and rejection or migration policy for unsupported bytes. |
| ADR-BL-003 | WAL, commit visibility, and `DurableCommitEvidence` | P0 | Transaction, WAL, storage, recovery, and observability owners | Accepted in `docs/adr/ADR-0015-wal-commit-visibility.md` | `AGENTS.md`, `ROADMAP_IMPLEMENTATION_2026.md`, and `WORKER_EXECUTION_MATRIX_2026.md` all require no visible commit without durable WAL. Worker 12 records explicit durable LSN evidence for commit and rollback terminal records. `CURRENT_STATE.md` says ProductStock prepared state remains invisible without durable commit evidence. | ADR-0015 defines the canonical evidence type, minimum fields, durability source, ordering rules, rollback evidence, audit/trace projection, recovery replay checks, and tests proving prepared state cannot become visible without durable evidence. |
| ADR-BL-004 | GPU exclusion from critical paths | P0 | Analytics/GPU, hardware, durable-kernel, and topology owners | Accepted in `docs/adr/ADR-0016-gpu-exclusion-critical-paths.md` | `AGENTS.md`, `README.md`, `ROADMAP_IMPLEMENTATION_2026.md`, `WORKER_EXECUTION_MATRIX_2026.md`, and `CURRENT_STATE.md` all keep GPU outside commit, rollback, WAL, recovery, MVCC visibility, and security-critical paths. ADR-0011 keeps durable kernel crates free of GPU dependencies. | ADR-0016 defines allowed GPU workloads, forbidden imports and call paths, CPU fallback, kill switches, trace evidence, topology tests, and failure behavior proving GPU absence cannot change contractual or recovered database results. |
| ADR-BL-005 | Rust toolchain and MSRV policy | P1 | Rust workspace and release governance owners | Accepted in `docs/adr/ADR-0017-rust-toolchain-msrv-policy.md` | `AGENTS.md` and ADR-0005 set Rust 2024 Edition as the baseline unless an ADR changes it. `Cargo.toml` sets `rust-version = "1.95.0"` and `edition = "2024"`. `rust-toolchain.toml` pins `channel = "1.95.0"`. `msrv-dependency-risk-2026-05-08.md` records dependency MSRV drift risk. | ADR-0017 confirms the active toolchain policy, edition, Rust 1.95.0 MSRV, dependency MSRV drift handling, lockfile update policy, CI and release gates, and required evidence before changing MSRV. |
| ADR-BL-006 | Macro-engine mapping and crate topology | P1 | Workspace architecture and release governance owners | Accepted in `docs/adr/ADR-0018-engine-crate-mapping-policy.md` | ADR-0011 defines R0 through R5 dependency rings and current crate ownership. `ROADMAP_IMPLEMENTATION_2026.md` adds WR-GOV-TOPO-DOC topology governance and keeps topology tests active. `WORKER_EXECUTION_MATRIX_2026.md` keeps anti-pattern crate-name and dependency drift checks active. | ADR-0018 maps macro-engines to crates, rings, planes, ownership boundaries, allowed dependencies, temporary facade exceptions, exit criteria, topology tests, and release-gate evidence. |

## Candidate Details

### ADR-BL-001 Unsafe Rust Policy

**Priority:** P0.

**Proposed owner:** Rust safety owner with release governance review.

**Disposition:** Accepted in `docs/adr/ADR-0013-unsafe-rust-policy.md`.

**Scope:** Workspace-wide unsafe policy for production crates, tests, fuzz
targets, generated code, FFI, SIMD intrinsics, OS calls, and future GPU
adapters.

**Acceptance criteria:**

- States whether production crates default to `#![forbid(unsafe_code)]`,
  `#![deny(unsafe_op_in_unsafe_fn)]`, or a narrower crate-by-crate policy.
- Requires every allowed unsafe block to have a local safety invariant and an
  owning reviewer.
- Defines forbidden unsafe locations, including commit, WAL durability,
  rollback, recovery replay, MVCC short-visibility, catalog publication, and
  security-critical admission unless a later ADR grants a narrower exception.
- Defines required validation by risk class: unit tests, property tests, fuzz,
  Miri, Loom, sanitizer builds, or manual audit.
- Defines escalation behavior for undocumented unsafe, panic-prone unsafe, FFI
  boundary drift, and generated-code unsafe.

**Residual risk:** ADR-0013 gives the canonical policy, but implementation
packets still need unsafe scans, Miri or audit evidence, and owner review before
any unsafe exception is accepted.

### ADR-BL-002 Binary Format And Endian Policy

**Priority:** P0.

**Proposed owner:** Binary format owner with storage, WAL, RPC, and release
governance review.

**Disposition:** Accepted in `docs/adr/ADR-0014-binary-format-endian-policy.md`.

**Scope:** Persistent bytes, WAL records, page images, manifests, cold
snapshots, catalog WAL records, RPC frames, Protobuf payload contracts, and
test-vector governance.

**Acceptance criteria:**

- Defines little-endian as the default canonical persistent binary format unless
  a specific format ADR records a different ordering.
- Records the RPC frame header network-byte-order exception and prevents it from
  becoming a storage-format precedent.
- Requires explicit format identity, version fields, length bounds, checksum or
  digest coverage, and unknown-version rejection behavior.
- Requires golden byte vectors for every durable format promoted beyond
  prototype status.
- Requires cross-architecture validation where endian behavior matters.
- Defines migration, rebuild, dual-read, or fail-closed policy for legacy bytes.

**Residual risk:** ADR-0014 gives the canonical policy, but each promoted
format still needs explicit codec implementation, golden vectors,
malformed-input rejection, and recovery or protocol compatibility evidence.

### ADR-BL-003 WAL, Commit Visibility, And `DurableCommitEvidence`

**Priority:** P0.

**Proposed owner:** Transaction kernel owner with WAL, storage, recovery, and
observability review.

**Disposition:** Accepted in `docs/adr/ADR-0015-wal-commit-visibility.md`.

**Scope:** Commit visibility, rollback terminal records, prepared state,
durable LSN reporting, recovery replay boundaries, ResultStream completion, and
audit or decision trace projections.

**Acceptance criteria:**

- Defines `DurableCommitEvidence` or its canonical equivalent by name and field
  set.
- Requires evidence to include transaction identity, commit or rollback terminal
  state, record LSN, durable LSN or durable-prefix boundary, WAL segment or
  generation identity, checksum or chain evidence, and source component.
- Defines when prepared mutations become visible and when they must remain
  invisible.
- Defines how recovery validates durable evidence and rejects incomplete,
  truncated, corrupt, or unauthorized terminal records.
- Defines how evidence appears in Procedure Store records, DecisionTrace,
  audit evidence, and ResultStream terminal metadata without making audit
  records database truth.
- Requires crash/recovery tests for durable commit, prepared-not-visible,
  rollback terminal evidence, WAL truncation, and replay boundary behavior.

**Residual risk:** ADR-0015 defines the documentation contract, but C5 runtime
paths still need retained WAL, transaction, storage, recovery, execution, audit,
and crash/recovery evidence before release claims.

### ADR-BL-004 GPU Exclusion From Critical Paths

**Priority:** P0.

**Proposed owner:** Analytics/GPU owner with durable-kernel, security, and
workspace topology review.

**Disposition:** Accepted in
`docs/adr/ADR-0016-gpu-exclusion-critical-paths.md`.

**Scope:** Current hardware policy crates, future optional GPU runtime crates,
statistics refresh, Map refresh, batch analytics, benchmark evidence, and vector
workloads.

**Acceptance criteria:**

- Defines allowed GPU use only for advisory or batch work: statistics refresh,
  Map refresh, batch analytics, bounded vector workloads, and diagnostic
  benchmarks.
- Forbids GPU dependencies, runtime initialization, device memory, kernels, and
  GPU-produced truth in commit, WAL append or flush, rollback, recovery, MVCC
  visibility, catalog publication, Procedure admission, authorization, and
  security-critical paths.
- Requires CPU fallback and a global disable path.
- Requires device, kernel, duration, transfer bytes, validation status, and
  fallback reason in trace evidence.
- Requires topology tests proving durable-kernel and security-critical crates do
  not import GPU runtime code.
- Requires validation proving GPU failure or disablement cannot alter
  contractual results or recovered database state.

**Residual risk:** ADR-0016 gives the placement policy, but future optional GPU
work still needs topology checks, CPU fallback tests, disablement tests, and
trace evidence before it can enter any advisory pipeline.

### ADR-BL-005 Rust Toolchain And MSRV Policy

**Priority:** P1.

**Proposed owner:** Rust workspace owner with release governance review.

**Disposition:** Accepted in
`docs/adr/ADR-0017-rust-toolchain-msrv-policy.md`.

**Scope:** Rust edition, stable-channel policy, MSRV, lint baseline, unsafe
linting, dependency constraints, and validation commands.

**Acceptance criteria:**

- Keeps ADR-0005 authoritative for Rust 2024 Edition.
- Records `Cargo.toml` `rust-version = "1.95.0"` as the current workspace MSRV.
- Records `rust-toolchain.toml` `channel = "1.95.0"` as the pinned
  release-validation toolchain.
- Defines dependency MSRV drift handling for packages that require a newer
  compiler.
- Defines when `Cargo.lock` updates are allowed and how they must be reviewed.
- Requires retained evidence before changing MSRV.
- Defines revision criteria for `rust-version`, pinned toolchain, future
  editions, dependency MSRV enforcement, and CI gate changes.

**Residual risk:** ADR-0017 resolves the policy ambiguity, but the current
dependency graph still needs retained Rust 1.95.0 release evidence until the
release owner resolves or validates the drift recorded in
`documentations/governance/msrv-dependency-risk-2026-05-08.md`.

### ADR-BL-006 Macro-Engine Mapping And Crate Topology

**Priority:** P1.

**Proposed owner:** Workspace architecture owner with release governance review.

**Disposition:** Accepted in
`docs/adr/ADR-0018-engine-crate-mapping-policy.md`.

**Scope:** Mapping from Andromeda macro-engines and planes to workspace crates,
dependency rings, compatibility facades, and topology validation.

**Acceptance criteria:**

- Maps each macro-engine to owning crates and rings: foundation, contracts,
  SRPL, catalog, durable kernel, transaction, execution, protocol, QUIC,
  security, observability, benchmark, analytics, tools, and future GPU.
- Identifies Application, Administration, Cluster, internal analytical, and
  recovery or forensic surfaces where relevant.
- Lists allowed dependency directions and forbidden edges.
- Records temporary facade exceptions and exit criteria.
- Defines topology tests and release gates that prove the map remains true.
- States how future crate splits update the map without creating generic
  `common`, `utils`, `misc`, `helpers`, or `god_engine` buckets.

**Residual risk:** ADR-0018 gives the macro-engine mapping policy, but the
dirty branch still needs clean-candidate metadata, topology tests, owner tests,
and facade exit evidence before crate topology can be treated as accepted
implementation state.

## Validation

This documentation-only backlog should be validated with targeted repository
inspection:

```powershell
rg -n "P12|Step 12|step 12|ADR backlog|unsafe policy|binary format|DurableCommitEvidence|macro-engine|macro engine|Rust baseline|Rust 2024|GPU exclusion|WAL-before|visible commit" documentations docs README.md AGENTS.md -S
rg -n "ADR-0013|ADR-0014|ADR-0015|ADR-0016|ADR-0017|ADR-0018|Accepted in" documentations/governance/adr-backlog-2026-05-08.md docs/adr -S
rg -n "step-12-documentation-closure-2026-05-08|spec-validation-matrix-2026-05-08" documentations -S
Test-Path documentations\governance\adr-backlog-2026-05-08.md
Test-Path docs\adr\ADR-0013-unsafe-rust-policy.md
Test-Path docs\adr\ADR-0014-binary-format-endian-policy.md
Test-Path docs\adr\ADR-0015-wal-commit-visibility.md
Test-Path docs\adr\ADR-0016-gpu-exclusion-critical-paths.md
Test-Path docs\adr\ADR-0017-rust-toolchain-msrv-policy.md
Test-Path docs\adr\ADR-0018-engine-crate-mapping-policy.md
```

No Rust build, Cargo test, or Codex tooling validation was required because this
change is documentation-only. It updates this backlog and Step 12 references
without editing Rust code, Cargo manifests, `Cargo.lock`, tooling, hooks,
skills, agents, or ADR-0005.

## Troubleshooting

| Symptom | Corrective action |
|---|---|
| A future ADR treats this backlog as acceptance authority. | Replace the claim with a citation to the accepted ADR or DEC that approves the behavior. |
| A future ADR places GPU in a critical path. | Escalate to doctrine review and reject the design unless project invariants are formally revised. |
| A future ADR serializes Rust structs directly to disk or network. | Require explicit codec, format identity, endian policy, and golden vectors. |
| A future ADR allows visible commit without durable WAL evidence. | Reject as a release blocker and require transaction, WAL, and recovery review. |
| A future ADR changes Rust baseline without validation policy. | Require toolchain, lint, CI, and revision criteria before acceptance. |

## References

- `AGENTS.md`
- `Cargo.toml`
- `README.md`
- `rust-toolchain.toml`
- `docs/adr/ADR-0005-rust-2024-baseline.md`
- `docs/adr/ADR-0013-unsafe-rust-policy.md`
- `docs/adr/ADR-0014-binary-format-endian-policy.md`
- `docs/adr/ADR-0015-wal-commit-visibility.md`
- `docs/adr/ADR-0016-gpu-exclusion-critical-paths.md`
- `docs/adr/ADR-0017-rust-toolchain-msrv-policy.md`
- `docs/adr/ADR-0018-engine-crate-mapping-policy.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `docs/adr/ADR-0012-quic-rpc-no-grpc.md`
- `documentations/CURRENT_STATE.md`
- `documentations/ROADMAP_IMPLEMENTATION_2026.md`
- `documentations/WORKER_EXECUTION_MATRIX_2026.md`
- `documentations/governance/msrv-dependency-risk-2026-05-08.md`
- `documentations/governance/decisions/DEC-026-release-gates-and-deferral-policy.md`
- `documentations/governance/decisions/DEC-029-e4-catalog-wal.md`
- `documentations/governance/decisions/DEC-032-storage-format-gate.md`
- `documentations/governance/step-12-documentation-closure-2026-05-08.md`
- `documentations/testing/spec-validation-matrix-2026-05-08.md`
- `documentations/specs/FrameHeader_RPC_v0.md`
