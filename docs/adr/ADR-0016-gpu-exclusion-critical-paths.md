# ADR-0016: GPU Exclusion From Critical Paths

## Status

Accepted

## Purpose

Record the canonical Andromeda decision for GPU placement and exclusion from
critical database paths.

GPU is excluded from commit, WAL, rollback, recovery, MVCC short visibility, catalog publication, and security-critical paths.
GPU output is advisory or batch evidence only; it is not database truth and must
not decide contractual, durable, recovered, or security-critical results.

This ADR allows future optional GPU work only when that work remains isolated
from the durable kernel and has CPU fallback, disablement, trace evidence, and
topology validation.

## Scope

This ADR applies to documentation and design acceptance for:

- current hardware policy crates;
- future optional GPU runtime crates;
- statistics refresh;
- Map refresh;
- batch analytics;
- bounded vector workloads;
- diagnostic benchmarks;
- dependency topology and import boundaries;
- trace evidence for GPU-assisted advisory work.

The exclusion applies to direct GPU runtime use, transitive GPU dependencies,
device memory ownership, kernel launch, GPU-produced truth, and any adapter that
would make a critical path wait for GPU completion.

## Non-goals

This ADR does not:

- implement GPU acceleration;
- select CUDA, ROCm, wgpu, Vulkan, DirectX, Metal, OpenCL, or any specific GPU
  provider;
- authorize GPU use in commit, WAL, rollback, recovery, MVCC short visibility,
  catalog publication, Procedure admission, authorization, or security-critical
  paths;
- authorize GPU output as catalog truth, storage truth, recovery truth,
  security truth, optimizer truth, or benchmark acceptance truth;
- change existing WAL, recovery, storage, catalog, transaction, or security
  runtime behavior;
- remove the requirement for CPU implementations of analytics, statistics, Map,
  vector, or benchmark paths;
- replace future topology, feature-flag, dependency, or failure-mode validation.

## Prerequisites

Before using this ADR as acceptance evidence, reviewers must understand these
active decisions and invariants:

- `AGENTS.md` forbids GPU work in commit, WAL, rollback, recovery, MVCC
  short-visibility, catalog publication, and security-critical paths.
- `docs/AGENTS.md` requires direct engineering documentation with validation
  and risk sections.
- ADR-0006 requires stronger validation for C4/C5 WAL, recovery, storage,
  security, catalog, and RPC changes.
- ADR-0011 keeps durable kernel crate boundaries explicit and prevents
  accidental dependency drift.
- `ROADMAP_IMPLEMENTATION_2026.md` keeps GPU optional, future-facing, and off
  critical paths.
- `WORKER_EXECUTION_MATRIX_2026.md` keeps no GPU on commit, WAL, rollback,
  recovery, MVCC visibility, or security-critical paths as active doctrine.
- `wal-pressure.md` states that WAL pressure must suspend non-critical GPU and
  analytics work before weakening durable WAL behavior.
- `adr-backlog-2026-05-08.md` identifies GPU exclusion from critical paths as a
  P0 ADR candidate.

## Decision

Andromeda accepts GPU acceleration only for optional, advisory, or batch work.
Allowed GPU work is limited to:

- statistics refresh candidates;
- Map refresh candidates that are outside transaction commit and publication
  truth;
- batch analytics;
- bounded vector workloads;
- diagnostic benchmark evidence.

Every allowed GPU path must have a CPU implementation that can produce the
authoritative result without GPU availability. GPU failure, disablement, timeout,
driver mismatch, device reset, or validation failure must not alter contractual
Procedure results, catalog publication, WAL durability, recovered database
state, authorization decisions, or MVCC visibility.

GPU code must live behind an optional boundary. Durable-kernel and
security-critical crates must not import GPU runtime code directly or
transitively. A future optional GPU crate must depend inward on stable contracts
or data snapshots; commit, WAL, rollback, recovery, MVCC, catalog publication,
and security-critical paths must not depend outward on that GPU crate.

GPU-produced data must be validated before publication to any advisory surface.
Accepted advisory data must record device, kernel or implementation identity,
duration, transfer bytes, validation status, fallback reason, input snapshot or
statistics version, and disablement state where applicable.

## Procedure

Use this ADR as the GPU placement acceptance checklist.

1. Classify the proposed GPU work as statistics refresh, Map refresh, batch
   analytics, bounded vector workload, diagnostic benchmark, or rejected.
2. Confirm that the proposed work is optional and advisory. Reject it if it can
   decide commit, WAL, rollback, recovery, MVCC short visibility, catalog
   publication, Procedure admission, authorization, or other security-critical
   behavior.
3. Confirm that an equivalent CPU path exists and remains the authoritative
   fallback.
4. Confirm that GPU runtime dependencies are isolated behind an optional crate,
   feature, or adapter boundary.
5. Confirm that durable-kernel and security-critical crates do not directly or
   transitively import GPU runtime code.
6. Confirm that GPU failure, disablement, timeout, or validation failure cannot
   change contractual results or recovered database state.
7. Confirm that trace evidence records device, kernel or implementation,
   duration, transfer bytes, validation status, fallback reason, and applicable
   snapshot or version identity.
8. Confirm that operators can disable GPU globally and, where needed, per
   advisory pipeline without weakening commit, WAL, recovery, catalog, MVCC, or
   security behavior.

## Validation

For this documentation-only ADR, validation is textual and structural:

- Confirm that the ADR states GPU is excluded from commit, WAL, rollback,
  recovery, MVCC short visibility, catalog publication, and security-critical
  paths.
- Confirm that the ADR treats GPU output as advisory or batch evidence, not as
  database truth.
- Confirm that the ADR requires CPU fallback and disablement.
- Confirm that the ADR requires dependency isolation for any future optional GPU
  runtime.
- Confirm that the ADR names future topology and failure-mode gates instead of
  claiming runtime behavior changed.

Implementation changes that add GPU-related code require stronger validation:

- Cargo feature and dependency topology tests proving durable-kernel and
  security-critical crates do not import GPU runtime code;
- CPU fallback tests for every GPU-assisted advisory result;
- failure-mode tests for GPU unavailable, timeout, validation failure, driver
  error, and explicit disablement;
- trace tests for device, kernel, duration, transfer bytes, validation status,
  fallback reason, and snapshot or version identity;
- tests proving GPU failure or disablement cannot alter Procedure results,
  visible commit, WAL durability, rollback, recovery, MVCC visibility, catalog
  publication, or authorization decisions.

Rust builds are not required for this ADR alone because the accepted artifact is
documentation-only. Any future GPU code, feature, dependency, topology, security,
storage, WAL, recovery, catalog, or transaction change must cite its owning
build, test, audit, and failure-mode evidence separately.

## Risks

- Optional GPU crates can drift into critical paths through transitive
  dependencies if topology tests are not kept active.
- Advisory GPU output can be overstated as truth unless ResultStream, catalog,
  optimizer, benchmark, and audit wording remain explicit.
- GPU initialization can create latency or availability coupling if it is
  performed on a path that should be able to proceed with CPU-only behavior.
- GPU validation can become a hidden requirement for catalog or statistics
  publication unless CPU fallback and disablement stay mandatory.
- Future hardware-specific optimizations can obscure failure behavior unless
  traces record validation status and fallback reason.

## Troubleshooting

Use this section when reviewing GPU, analytics, statistics, Map, benchmark, or
hardware documentation.

| Symptom | Corrective action |
| --- | --- |
| A document places GPU in commit, WAL, rollback, recovery, MVCC short visibility, catalog publication, or a security-critical path. | Reject the design unless a later project-wide ADR formally revises the invariant. |
| A document says GPU output is database truth. | Reword GPU output as advisory or batch evidence and require CPU validation or fallback. |
| A proposed durable-kernel crate imports a GPU runtime dependency. | Move GPU code behind an optional advisory crate and add topology validation. |
| A GPU path has no CPU fallback. | Reject the path until an authoritative CPU implementation exists. |
| A GPU failure changes a Procedure result, visible commit, recovery result, catalog publication, or authorization decision. | Treat it as a release blocker and restore critical behavior to CPU-backed durable paths. |
| A document omits disablement or trace evidence. | Require global disablement and trace fields for device, kernel, duration, transfer bytes, validation status, fallback reason, and snapshot or version identity. |

## References

- `AGENTS.md`
- `docs/AGENTS.md`
- `docs/adr/ADR-0006-mission-critical-validation.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `documentations/ROADMAP_IMPLEMENTATION_2026.md`
- `documentations/WORKER_EXECUTION_MATRIX_2026.md`
- `documentations/governance/adr-backlog-2026-05-08.md`
- `documentations/operations/runbooks/wal-pressure.md`
- `documentations/operations/benchmarking.md`
- `documentations/specs/DefinitionBatch_v0.md`
- `documentations/specs/CatalogObjectModel_v0.md`
