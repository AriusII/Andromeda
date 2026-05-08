# StatsObject v0 Specification

## Purpose

Define the accepted documentation contract for `StatsObject v0`, the bounded
statistics catalog object used by the optimizer to estimate cardinality, cost,
selectivity, skew risk, and plan stability.

`StatsObject v0` is advisory optimizer evidence. It is not database truth, not
a replacement for typed Procedure contracts, and not permission to override SRPL
cardinality, nullability, transaction, catalog, or security semantics.

## Scope

This specification applies to statistics documentation for cataloged tables,
maps, access paths, column sets, and future analytics-derived summaries.

It covers:

- `StatsVersion` identity and publication rules;
- histogram, NDV, density, skew, and correlation evidence;
- publication states and validation boundaries;
- advisory-only optimizer consumption;
- `DecisionTrace` and plan-cache links;
- extraction gaps and uncertainty reporting;
- stale, rejected, superseded, or incomplete statistics behavior.

## Current Implementation Status

The repository already contains `StatsVersion` as part of Procedure contract
binding, plan-cache identity, optimizer evidence, and documentation for
candidate-to-published statistics. This specification is the target
documentation contract for the `StatsObject v0` shape and lifecycle. It does not
claim that every metric listed here is fully implemented in a runtime statistics
engine.

Future implementation work must either encode this object directly or map
existing statistics evidence into an equivalent typed catalog object without
weakening versioning, publication, advisory-use, or trace requirements.

## Non-goals

This specification does not:

- define a full statistics collection scheduler;
- define a physical catalog byte format;
- define every histogram algorithm or bucket construction algorithm;
- make statistics exact database truth;
- allow optimizer estimates to override SRPL cardinality contracts or Procedure
  result shapes;
- allow GPU output, learned component output, benchmark output, or temp state to
  publish truth without validation;
- introduce dynamic table names, dynamic predicates, shape-shifting returns, or
  implicit null semantics;
- introduce gRPC, runtime JSON defaults, ad hoc SQL, generic command text, or
  untyped statistics payloads.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `documentations/05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md` for
  statistics catalog fields, publication states, and optimizer trace evidence.
- `documentations/02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md` for table
  `CurrentStatsVersion`, Procedure plan identity, maps, and analytical
  summaries.
- DEC-016 for plan-cache key identity over `ProcedureId`, `ContractHash`,
  `CatalogVersion`, `StatsVersion`, `PolicyVersion`, plan class, and shape
  evidence.
- DEC-039 for advisory cost estimates and `DecisionTrace` requirements.
- `documentations/specs/AuditLedger_v0.md` for forensic evidence boundaries
  when statistics publication or optimizer decisions are audited.

## Procedure

### Object model

`StatsObject v0` must be a bounded, typed catalog object. Documentation may use
field names that match local implementation naming, but it must preserve the
following semantics:

| Field group | Required evidence |
| --- | --- |
| Identity | `StatsId`, `ObjectId`, object kind, `CatalogVersion`, `StatsVersion`, column set, source snapshot id. |
| Scope | Table, map, access path, column, column group, predicate class, partition or segment scope when applicable. |
| Cardinality evidence | Row count basis, sample count, NDV estimate or exact marker, null or absent count, extraction coverage. |
| Distribution evidence | Histogram kind, bucket boundaries, bucket counts or frequencies, density, skew score, heavy hitters when available. |
| Correlation evidence | Column-group correlation hints, functional dependency hints, predicate-correlation gaps, confidence. |
| Freshness evidence | Collected-at timestamp, validated-at timestamp, source LSN or snapshot, staleness policy, drift markers. |
| Publication evidence | Publication state, validation state, publisher, rejection reason, superseded-by version when applicable. |
| Trace links | Collection trace id, validation trace id, publication `DecisionTrace` id, consuming plan `DecisionTrace` ids when available. |

`StatsObject v0` must not store raw row bodies, secret-bearing values,
unbounded sample payloads, or application command text. Value samples must be
bounded, sanitized, and typed, or represented by digest and bucket evidence.

### StatsVersion

`StatsVersion` is the published statistics identity consumed by Procedure
planning and plan-cache identity. It must be:

- nonzero when used by a plan-cache key or Procedure contract binding;
- monotonic within its catalog publication scope;
- tied to a `CatalogVersion` and source snapshot or source LSN;
- immutable after publication;
- recorded by every plan that consumes it;
- invalidated or separated from cached plans when it changes.

A candidate `StatsVersion` must not become current until validation and
controlled publication succeed. A rejected candidate must remain traceable by
its rejection reason and validation trace, but it must not become active.

### Histograms

Histograms describe value distribution for a column or column set. They are
optimizer evidence and must be explicit about their construction limits.

| Requirement | Description |
| --- | --- |
| Histogram kind | Equi-depth, equi-width, singleton-heavy-hitter, hybrid, or implementation-defined bounded kind. |
| Bucket identity | Stable bucket order, lower and upper bounds when meaningful, inclusive or exclusive boundary semantics. |
| Bucket evidence | Row count or frequency, NDV per bucket when available, null or absent handling outside normal buckets. |
| Coverage | Exact, sampled, map-derived, access-path-derived, or partial extraction coverage. |
| Confidence | Confidence score or uncertainty class when sampled or partially extracted. |

Histogram documentation must not imply implicit null semantics. Null, absent,
unknown, and not-collected states must remain distinct according to the
Andromeda type system and StructuredObject rules.

### NDV, density, and skew

NDV means number of distinct values for the declared statistic scope. The object
must distinguish exact NDV from estimated NDV.

| Measure | Required interpretation |
| --- | --- |
| `RowCount` | Count basis used by the statistic. It may be exact-at-snapshot, sampled, or inherited from a validated map. |
| `NDV` | Distinct value count for the declared scope, with exact or estimated marker. |
| `Density` | Selectivity evidence derived from NDV, buckets, or observed frequencies; never a cardinality contract. |
| `SkewScore` | Bounded indicator that distribution is non-uniform enough to affect plan risk. |
| `HeavyHitters` | Optional bounded list of dominant values or digests when needed for skew-aware costing. |
| `CorrelationHints` | Advisory evidence that independent-selectivity assumptions may be unsafe. |

Density and skew can increase optimizer risk penalty, trigger alternate plan
classes, or require stronger trace explanations. They must not change declared
Procedure result shape or SRPL cardinality.

### Publication states

`StatsObject v0` must use explicit publication states.

| State | Meaning | Optimizer use |
| --- | --- | --- |
| `Candidate` | Collected but not validated. | Not active; may be compared in validation only. |
| `Validating` | Under consistency, coverage, and drift checks. | Not active except in isolated validation. |
| `Published` | Current or consumable version passed publication gates. | May be used as advisory evidence and must be traced. |
| `Rejected` | Failed validation, policy, or consistency gates. | Must not be used for planning. |
| `Expired` | Too stale for normal planning policy. | Must not be used unless a fail-safe policy explicitly allows stale advisory use with trace. |
| `Superseded` | Replaced by a newer published version. | Must not be used for new plans; may remain linked to historical plans. |

Publication must be atomic from the optimizer perspective. A plan must consume
one coherent `StatsVersion`; it must not mix partial publication from multiple
candidate versions unless a separate documented composite version defines that
identity.

### Advisory-only optimizer use

The optimizer may use `StatsObject v0` for:

- cardinality estimation;
- selectivity estimation;
- join order and access path costing;
- plan-class selection;
- risk penalties for uncertainty, skew, stale stats, or extraction gaps;
- plan stability and hysteresis explanations.

The optimizer must not use statistics to:

- override SRPL cardinality contracts;
- change typed Procedure input or output shapes;
- bypass `ContractHash`, `CatalogVersion`, `PolicyVersion`, or permission
  checks;
- make a commit, rollback, WAL, recovery, catalog, or security decision;
- treat GPU, learned model, benchmark, or temp output as truth;
- reuse a cached plan across a changed `StatsVersion` without a distinct cache
  identity.

When statistics are missing, stale, or rejected, the optimizer must use a
bounded fallback and record uncertainty in `DecisionTrace`.

### DecisionTrace links

Every optimizer decision that materially depends on `StatsObject v0` must be
explainable after the fact. The relevant `DecisionTrace` must record:

- consumed `StatsVersion`;
- relevant `StatsId` values or a digest of the consumed statistics set;
- histogram, NDV, density, skew, stale-stat, and extraction-gap evidence that
  affected the choice;
- candidate plans considered and rejected;
- cost estimates and risk penalties;
- fallback reason when statistics were missing, rejected, expired, or too
  uncertain;
- final plan choice and stability or hysteresis reason when applicable.

Plan-cache hit, miss, insertion, eviction, invalidation, stats-drift warning,
and cost-accuracy alert decisions must preserve `StatsVersion` and
`DecisionTrace` evidence as required by the plan-cache contract.

### Extraction gaps

`StatsObject v0` must make missing or partial statistics explicit.

| Gap | Required representation | Planning behavior |
| --- | --- | --- |
| `NotCollected` | No statistic exists for the requested scope. | Use bounded fallback and trace missing evidence. |
| `PartialCoverage` | Statistic covers only a sample, segment, map, or predicate subset. | Apply confidence or risk penalty and trace coverage. |
| `Stale` | Source changed beyond policy threshold. | Prefer refresh or fallback; trace stale use if policy permits it. |
| `UnknownCorrelation` | Multi-column dependency is not known. | Avoid claiming independent selectivity as fact; increase uncertainty. |
| `NullAbsentUnmeasured` | Null or absent counts were not collected. | Keep null and absent semantics unresolved; do not infer implicit null behavior. |
| `SkewUnmeasured` | Heavy hitters or skew score were not collected. | Avoid skew-sensitive plan confidence; trace uncertainty. |
| `ValidationFailed` | Collection exists but failed checks. | Do not use for new planning. |

Extraction gaps are first-class evidence. They are not warnings to hide in prose
after the plan decision is made.

## Validation

Documentation acceptance checks:

- The spec describes `StatsObject v0` as advisory optimizer evidence, not
  database truth.
- The spec requires `StatsVersion` identity, publication validation, and plan
  consumption tracking.
- The spec covers histograms, NDV, density, skew, and correlation evidence.
- The spec defines `Candidate`, `Validating`, `Published`, `Rejected`,
  `Expired`, and `Superseded` states.
- The spec states that statistics cannot override SRPL cardinality contracts or
  typed Procedure shapes.
- The spec requires `DecisionTrace` links for optimizer and plan-cache decisions
  that use statistics.
- The spec requires extraction gaps to be represented and traced.
- The spec does not introduce gRPC, runtime JSON defaults, ad hoc SQL, dynamic
  table names, dynamic predicates, or shape-shifting returns.

Future implementation work should add or keep targeted validation for:

```powershell
cargo test -p andromeda-catalog --test statistics_builder_tests
cargo test -p andromeda-catalog --test stats_publication_switch_tests
cargo test -p andromeda-catalog plan_cache
```

These commands are not required for documentation-only acceptance.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Plan cache reuses a plan after `StatsVersion` changes. | Cache identity ignored statistics versioning. | Separate or invalidate by complete plan-cache key identity. |
| Cost estimate changes Procedure cardinality. | Statistics were treated as semantic truth. | Restore SRPL cardinality contract authority and keep stats advisory. |
| Published stats have no validation trace. | Candidate publication skipped evidence. | Reject publication or add validation and publication `DecisionTrace` links. |
| Histogram implies nulls are part of a normal value bucket. | Null, absent, and unknown states were collapsed. | Represent null and absent counts explicitly or mark them unmeasured. |
| Optimizer hides missing statistics. | Extraction gap was not represented. | Emit fallback reason, confidence, and risk penalty in `DecisionTrace`. |
| GPU or learned output becomes current stats directly. | Advisory accelerator output bypassed validation. | Require CPU-verifiable validation and controlled publication before use. |

## References

- `documentations/02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md`
- `documentations/05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md`
- `documentations/governance/decisions/DEC-016-plan-cache-scope.md`
- `documentations/governance/decisions/DEC-039-optimizer-intermediate-pass-contract.md`
- `documentations/specs/AuditLedger_v0.md`
- `crates/andromeda-catalog/src/`
- `crates/andromeda-observe/src/events/decision.rs`
