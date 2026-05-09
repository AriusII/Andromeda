# Advisory Optimizer And Hardware

## Purpose

This spec defines statistics, plan cache identity, Map descriptors, Map refresh
validation, GPU policy, and hardware acceleration boundaries. These systems may
advise planning and analytics. They must not become database truth, security
authority, WAL authority, recovery truth, or transaction visibility authority.

## Statistics

Statistics are versioned advisory evidence. `StatsVersion` identifies published
statistics for planning but does not change SRPL cardinality semantics,
Procedure contracts, or source truth.

| Field group | Required evidence |
| --- | --- |
| Identity | `StatsId`, object id, object kind, `CatalogVersion`, `StatsVersion`, column set, source snapshot id. |
| Scope | Table, map, access path, column, column group, predicate class, partition, or segment scope. |
| Cardinality | Row count basis, sample count, NDV estimate or exact marker, null or absent count, extraction coverage. |
| Distribution | Histogram kind, bucket bounds, bucket counts or frequencies, density, skew score, heavy hitters. |
| Freshness | Collected-at, validated-at, source LSN or snapshot, staleness policy, drift markers. |
| Publication | Publication state, validation state, publisher, rejection reason, superseded version. |
| Trace links | Collection, validation, publication, and consuming plan decision traces. |

Published statistics may be used only as advisory evidence and must be traced.
Candidate, validating, rejected, expired, superseded, missing, stale, partial,
or unmeasured evidence must be represented explicitly and must not be hidden as
fact.

## Plan cache key

`PlanCacheKey v0` is exactly this identity tuple:

| Field | Rule |
| --- | --- |
| `ProcedureId` | Nonzero identity of the cataloged Procedure. |
| `ContractHash` | Nonzero deterministic hash of the typed Procedure contract. |
| `CatalogVersion` | Nonzero catalog publication version used for binding. |
| `StatsVersion` | Nonzero published statistics version consumed by planning. |
| `PolicyVersion` | Nonzero policy identity for permissions and resource policy. |
| `PlanClass` | Closed bounded class explaining why multiple plans may exist. |
| `PlanShapeFingerprint` | Bounded fingerprint over typed shape and cardinality evidence. |

No other field participates in `PlanCacheKey v0` identity. Future axes require
a new spec version. Cache hits require exact key equality and matching key
digest.

| Bound | Requirement |
| --- | --- |
| Candidate count | Reject more than 8 plan candidates. |
| Advisory evidence count | Reject more than 8 scenario evidence records. |
| Cache capacity | Nonzero and capped; current maximum is 64 entries. |
| Eviction | Deterministic and traceable; current policy evicts the oldest entry when full. |
| Plan digest | Stored entries carry a nonzero digest supplied by compiler or optimizer. |

Cache disablement must gate reuse before lookup or make lookup trace-only.
Benchmark and scenario evidence must remain advisory, expirable, bounded, and
explainable.

## Map descriptor

Maps describe materialized or analytical derived state. A Map is not source
truth unless its source, lineage, publication, and durability gates explicitly
make the derived state valid for the requested use.

| Field group | Required evidence |
| --- | --- |
| Identity | Map id or catalog object id, object kind `Map`, qualified name, namespace, `CatalogVersion`, `MapVersion`, descriptor schema version. |
| Definition | Canonical typed definition IR digest, source Procedure or SRPL binding, result columns, output StructuredObject contracts. |
| Grain | Grain key, row meaning, uniqueness rule, dimensions, time bucket or snapshot scope. |
| Summarizability | Aggregate class, dimension hierarchy, duplicate policy, empty-set behavior, null and absent handling. |
| Refresh policy | Mode, trigger, maximum apply cost, maximum commit cost when immediate, retry, fail-closed behavior. |
| Staleness | Maximum age, source LSN lag, mutation count or percentage, snapshot boundary, stale-use policy. |
| Lineage | Source object ids and versions, source snapshot or WAL range, delta range, refresh job id, validation trace. |
| Publication | Candidate state, validation result, durable WAL evidence, active pointer transition, rejection reason. |

Map publication states are `Candidate`, `Validating`, `WalRecorded`,
`Published`, `Rejected`, `Expired`, and `Superseded`. Candidate and validating
states are not visible for Procedure planning or execution. `WalRecorded` is
not visible until durable WAL coverage is confirmed.

Immediate refresh is allowed only with strict bounded commit cost and same
transaction atomicity. Large analytical refresh should use incremental,
deferred, or snapshot-only modes.

## Map refresh validation

Map refresh validation must check refresh policy, source lineage, grain,
summarizability, staleness, current catalog stats, durable publication, and
plan-cache impact.

Diagnostics include missing refresh policy, unbounded immediate cost, stale
stats, unproven summarizability, stats not published, stats not durable,
analytics treated as truth, and missing decision trace.

Optimizer use of a Map must record use, rejection, stale use, summarizability,
fallback, and risk evidence. Map or stats policy changes must advance catalog,
stats, or policy identity so plan-cache reuse separates through a new key.

## GPU and hardware policy

GPU and hardware acceleration output is advisory unless validated and published
through the owning domain's durable gates.

| Job class | Allowed use | Output authority |
| --- | --- | --- |
| `GPU_STATS` | Histograms, cardinality estimates, density, skew, and distribution candidates. | Candidate statistics evidence only. |
| `GPU_ANALYTICS` | Snapshot-only analytical Map scans and large aggregations. | Advisory analytical result or Map candidate only. |
| `GPU_BENCHMARK` | Predictive benchmark and scenario-evidence computation. | Scenario evidence only. |
| `GPU_VECTOR` | Controlled future vector or similarity extensions. | Advisory extension evidence only. |
| `GPU_COMMIT` | Commit, visible decision, or transaction terminal behavior. | Forbidden. |
| `GPU_WAL` | WAL append, flush, replay, checksum authority, or retention. | Forbidden. |
| `GPU_ROLLBACK` | Undo, rollback, poison handling, or transaction cleanup authority. | Forbidden. |
| `GPU_RECOVERY` | Startup recovery, redo, corruption boundary, manifest selection, or restore truth. | Forbidden. |
| `GPU_SECURITY` | Identity, permission, policy, admission, certificate, or audit authorization. | Forbidden. |
| `GPU_CATALOG_PUBLICATION` | Catalog switch, stats publication, Map publication, or Procedure contract publication. | Forbidden. |
| `GPU_MVCC_VISIBILITY` | Snapshot visibility or row-level concurrency decisions. | Forbidden. |

GPU jobs must use immutable snapshot or candidate evidence. They must not hold
live transaction locks, mutable page handles, WAL writer handles, catalog
publishers, or security decision handles.

CPU fallback must be exact, bounded approximate with validation, or safe
decline with an extraction gap. WAL pressure, recovery, shutdown, or critical
path pressure may cancel or suspend GPU work without changing contractual or
recovered results.

## Validation gates

- Stats publication tests must reject candidate, stale, expired, superseded,
  or unvalidated evidence for active planning.
- Plan-cache tests must prove exact identity matching and invalidation on
  catalog, stats, policy, class, or shape changes.
- Map tests must prove grain, summarizability, staleness, durable publication,
  and lineage checks.
- GPU tests must prove no critical commit, WAL, rollback, recovery, security,
  catalog publication, or MVCC visibility path depends on accelerator output.
