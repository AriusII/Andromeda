# GpuBatchPolicy v0 Specification

## Purpose

Define the accepted documentation contract for `GpuBatchPolicy v0`, the
advisory GPU batch boundary for Andromeda analytics, statistics, benchmark, and
future vector workloads.

`GpuBatchPolicy v0` is an off-critical-path acceleration policy. GPU output is
candidate evidence only. It is not database truth, not durable state, not a
security decision, and not a transaction, WAL, rollback, recovery, MVCC,
catalog-publication, or visible-commit dependency.

## Scope

This specification applies to documentation and future implementation work that
mentions GPU execution, GPU scheduling, GPU-assisted statistics, analytical map
scans, benchmark scenario evaluation, or vector-style batch acceleration.

It covers:

- allowed and forbidden GPU job classes;
- advisory-only output authority;
- CPU fallback requirements;
- cancellation and backpressure behavior;
- validation before publication of any derived statistics or analytical
  evidence;
- trace, metric, and decision-evidence requirements;
- failure behavior when GPU hardware, drivers, memory, queues, or validation
  are unavailable.

## Current Implementation Status

The roadmap documents GPU as a future optional accelerator for statistics,
analytics, benchmark scenarios, and controlled vector extensions. The current
specification is the policy target for that work. It does not claim that a GPU
runtime scheduler, device adapter, driver integration, or GPU execution engine
is complete.

Existing documentation already establishes the governing doctrine:

- GPU is allowed for histograms, cardinality, distribution, skew detection,
  analytical scans, large aggregations, benchmark scenarios, and controlled
  vector extensions.
- GPU is forbidden in commit, WAL, rollback, recovery, row-by-row OLTP lookup,
  short MVCC visibility, Procedure OLTP logic, and security-critical behavior.
- Published statistics must be validated and versioned before becoming active.

## Non-goals

This specification does not:

- make GPU required for correctness, availability, admission, execution, commit,
  rollback, WAL, recovery, catalog publication, or security;
- allow GPU output to become `StatsVersion`, map, catalog, Procedure, audit, or
  storage truth without CPU-verifiable validation and controlled publication;
- define a concrete CUDA, ROCm, wgpu, Vulkan, Metal, or DirectX adapter;
- define GPU kernel source code, shader bytecode, driver packaging, or FFI
  ownership rules;
- authorize unbounded device memory allocation, unbounded queues, or hidden
  background work;
- expose GPU controls through the Application Surface;
- introduce gRPC, runtime JSON defaults, ad hoc SQL, generic command text, or
  untyped payload tunnels;
- serialize Rust native structs directly to disk, network, or GPU-persisted
  artifacts.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `AGENTS.md` for the no-GPU-critical-path invariant.
- `documentations/05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md`
  for GPU job classes and optimizer/statistics roadmap context.
- `documentations/02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md` for
  StructuredObject layouts, maps, analytics, and off-commit GPU placement.
- `documentations/specs/StatsObject_v0.md` for `StatsVersion` publication and
  advisory statistics behavior.
- `documentations/specs/AuditLedger_v0.md` for forensic evidence boundaries.
- `documentations/operations/runbooks/wal-pressure.md` for operational
  suspension of analytics and GPU batches under WAL pressure.

## Procedure

### Policy contract

GPU work must be optional, bounded, observable, explainable, versioned where it
feeds cataloged evidence, and disableable by policy. A GPU path may accelerate a
batch computation, but the CPU-owned runtime must retain the authority to
validate, reject, cancel, retry, fall back, or ignore the result.

The accepted v0 GPU job classes are:

| Job class | Allowed use | Priority | Output authority |
| --- | --- | ---: | --- |
| `GPU_STATS` | Histograms, cardinality estimates, density, skew, and distribution candidates. | Low to medium | Candidate statistics evidence only. |
| `GPU_ANALYTICS` | Snapshot-only analytical map scans and large aggregations. | Low | Advisory analytical result or map candidate only. |
| `GPU_BENCHMARK` | Predictive benchmark and scenario-evidence computation. | Low | Scenario evidence only. |
| `GPU_VECTOR` | Controlled future vector or similarity extensions. | Low | Advisory extension evidence only. |
| `GPU_COMMIT` | Commit, visible decision, or transaction terminal behavior. | N/A | Forbidden. |
| `GPU_WAL` | WAL append, WAL flush, WAL replay, WAL checksum authority, or WAL retention. | N/A | Forbidden. |
| `GPU_ROLLBACK` | Undo, rollback, poison handling, or transaction cleanup authority. | N/A | Forbidden. |
| `GPU_RECOVERY` | Startup recovery, redo, corruption-boundary classification, manifest selection, or restore truth. | N/A | Forbidden. |
| `GPU_SECURITY` | Identity, permission, policy, admission, certificate, or audit authorization decisions. | N/A | Forbidden. |
| `GPU_CATALOG_PUBLICATION` | Catalog switch, `StatsVersion` publication, map publication, or Procedure contract publication. | N/A | Forbidden. |
| `GPU_MVCC_VISIBILITY` | Short-visibility checks, snapshot visibility, or row-level concurrency decisions. | N/A | Forbidden. |

Any job that cannot be classified into an allowed class must be rejected before
queueing. Forbidden classes must not be implemented as "high priority" jobs,
feature flags, emergency fallbacks, or hidden maintenance work.

### Job descriptor

A GPU batch job must be described by bounded typed evidence before dispatch:

| Evidence | Required rule |
| --- | --- |
| Job identity | `GpuJobId`, job class, enqueue timestamp, request or trace correlation when available. |
| Source identity | `ObjectId`, map identity, `CatalogVersion`, source snapshot id or source LSN, and `StatsVersion` when applicable. |
| Input shape | Typed descriptor, row or column count, byte size, layout, batch count, and sanitized value-domain evidence. |
| Policy evidence | `PolicyVersion`, hardware profile, resource class, priority, timeout, cancellation token, and CPU fallback mode. |
| Isolation evidence | Snapshot-only or candidate-only source. No live transaction locks, mutable page handles, WAL writer handles, catalog publishers, or security decision handles. |
| Output contract | Expected shape, validation tolerance, checksum or digest evidence, and publication target if any. |
| Trace evidence | `DecisionTrace` or equivalent GPU batch trace id, fallback reason, validation result, and cancellation outcome. |

The descriptor must not include secret-bearing values, raw credentials, private
keys, unbounded row bodies, unbounded samples, generic command text, SQL text,
or native Rust memory-layout assumptions.

### Dispatch order

GPU dispatch must follow this order:

1. Classify the requested work into an allowed GPU job class.
2. Reject forbidden classes before queueing.
3. Bind the job to immutable snapshot, candidate statistics, scenario, or
   analytical-map evidence.
4. Assign a bounded resource budget, queue priority, timeout, and cancellation
   token.
5. Verify that a CPU scalar or CPU SIMD fallback exists for correctness, or that
   the job can be safely marked as not collected without weakening semantics.
6. Dispatch the GPU job only when policy, capacity, and backpressure allow it.
7. Validate GPU output with CPU-owned checks before any publication or
   optimizer consumption.
8. Publish only through the normal CPU-owned candidate validation and controlled
   switch path.
9. Record trace evidence for success, fallback, cancellation, validation
   failure, or rejection.

No transaction may wait for GPU completion to become visible. No WAL record may
depend on GPU completion. No recovery, rollback, security, or catalog
publication path may block on GPU work.

### CPU fallback

Every allowed GPU job must have one of these fallback modes:

| Fallback mode | Requirement |
| --- | --- |
| `CpuExact` | CPU scalar or CPU SIMD path can compute equivalent evidence exactly. |
| `CpuApproximateValidated` | CPU path can validate a bounded approximate result and record confidence or tolerance. |
| `CpuDeclineWithGap` | CPU path can safely decline the job, mark the statistic or analytical evidence as missing, and record an extraction gap. |

Fallback must activate when:

- GPU is disabled by policy;
- hardware, driver, or runtime detection fails;
- a queue is saturated;
- WAL, recovery, security, catalog, or foreground resource pressure requires
  suspension of non-critical work;
- the job times out;
- the job is canceled;
- device memory cannot be allocated within budget;
- GPU output fails shape, checksum, tolerance, determinism, or CPU validation;
- GPU output conflicts with `Procedure Store`, `StatsObject`, map, catalog, or
  policy evidence.

Fallback must preserve user-visible correctness. The worst accepted outcome for
statistics or analytics is missing, stale, lower-confidence, or delayed
advisory evidence with a traceable reason. It is never a visible commit,
security bypass, recovery dependency, or catalog publication shortcut.

### Cancellation and backpressure

GPU jobs must be cancellable before launch and at documented batch boundaries.
If a device runtime cannot preempt an in-flight kernel, cancellation must mark
the job result as unusable unless the completion is validated after the
cancellation boundary and policy still permits consumption.

Cancellation must be safe:

- canceling a GPU job must not require transaction rollback;
- canceling a GPU job must not alter WAL, recovery, security, MVCC, or catalog
  truth;
- canceling a GPU job must not leak device memory or leave later jobs with
  cross-principal data;
- canceled jobs must emit bounded trace evidence;
- GPU queues must shed work before WAL, recovery, audit, security admission,
  catalog publication, or foreground Procedure execution is starved.

When WAL pressure, recovery pressure, security pressure, catalog publication
pressure, or incident response is active, policy must be able to pause or drain
GPU batches without reducing database correctness.

### Output validation and publication

GPU output has no publication authority by itself. A CPU-owned validator must
check:

- output shape, row count, column count, and descriptor identity;
- source snapshot, `CatalogVersion`, `StatsVersion`, and `PolicyVersion`
  compatibility;
- checksum, digest, or deterministic recomputation evidence;
- tolerance and confidence for approximate algorithms;
- absence of implicit null, absent, or unknown-state collapse;
- bounded output size and memory ownership;
- stale input, canceled job, timeout, or superseded candidate status.

Statistics publication must still follow the `StatsObject v0` candidate,
validation, and controlled switch lifecycle. Analytical map publication must
still follow map refresh and publication rules. Benchmark output remains
scenario evidence. Vector output remains advisory extension evidence until a
separate reviewed contract grants more specific semantics.

### Observability

GPU batch work must produce evidence that can explain why a result was used,
ignored, canceled, or replaced by fallback.

At minimum, trace or metric evidence must include:

- job class and policy version;
- hardware profile and selected adapter class;
- source snapshot, catalog, map, or statistics identity;
- queue delay, runtime duration, timeout, and cancellation status;
- input size, output size, and batch count;
- fallback mode and fallback reason when used;
- validation result and validation reason;
- publication candidate id or rejection reason when applicable.

GPU traces are operational and decision evidence. They are not database truth
and do not replace durable WAL, catalog, storage, or security audit evidence.

### Security and isolation

GPU adapters must be treated as untrusted acceleration boundaries. They must not
own identity, permission, policy, admission, audit authorization, certificate,
break-glass, or surface-routing decisions.

Future implementations must isolate device memory between jobs and principals,
avoid secret-bearing inputs, clear or discard reusable buffers according to the
security policy, and reject GPU work that would require raw credential material
or unbounded application payloads on the device.

## Validation

Documentation acceptance checks:

- The spec states that GPU work is advisory and off-critical-path only.
- The spec forbids GPU participation in commit, WAL, rollback, recovery, MVCC
  short visibility, catalog publication, and security-critical paths.
- The spec requires CPU fallback or safe decline for every allowed GPU job.
- The spec requires cancellation without transaction, recovery, catalog, or
  security side effects.
- The spec requires CPU-owned validation before publishing statistics,
  analytical map evidence, benchmark evidence, or vector evidence.
- The spec does not make GPU output database truth.
- The spec does not introduce gRPC, runtime JSON defaults, ad hoc SQL, generic
  command text, or untyped payload tunnels.

Future implementation work should add or keep targeted validation for:

```powershell
cargo test -p andromeda-catalog --test statistics_builder_tests
cargo test -p andromeda-catalog --test stats_publication_switch_tests
cargo test -p andromeda-observe --test decision_trace_contract
```

GPU runtime implementation must add dedicated tests for disabled-GPU fallback,
queue saturation fallback, cancellation, timeout, validation failure, stale
source rejection, and no-critical-path drift before release promotion.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| A transaction waits for GPU work before becoming visible. | GPU was placed on a critical transaction path. | Remove the GPU dependency and use CPU-owned transaction logic only. |
| WAL pressure increases while GPU jobs keep running. | Non-critical GPU work was not paused under pressure. | Suspend GPU batches and record the operational decision trace. |
| Published statistics came directly from GPU output. | Candidate validation and controlled publication were bypassed. | Revoke the publication, recompute or validate on CPU, and publish through `StatsObject v0` gates. |
| GPU cancellation requires rollback. | The job captured live transaction state. | Redesign the job to use immutable snapshot or candidate evidence only. |
| GPU validation accepts a shape mismatch. | Output contract or CPU validation is incomplete. | Reject the result and add descriptor, row-count, and digest checks. |
| Application clients can start GPU jobs. | GPU controls leaked onto the Application Surface. | Move controls to the owning administrative or internal scheduler path and require policy. |
| GPU output is used for permission or admission decisions. | Security authority drifted into an accelerator. | Reject the path and restore CPU-owned typed identity, surface, contract, and permission evaluation. |

## References

- `AGENTS.md`
- `documentations/05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md`
- `documentations/02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md`
- `documentations/specs/StatsObject_v0.md`
- `documentations/specs/AuditLedger_v0.md`
- `documentations/operations/runbooks/wal-pressure.md`
- `.agents/skills/gpu-no-commit-policy/SKILL.md`
