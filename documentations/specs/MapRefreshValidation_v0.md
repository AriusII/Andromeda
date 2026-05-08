# MapRefreshValidation v0 Specification

## Purpose

Define the accepted validation contract for Map refresh, Map analytics, and
current catalog stats publication.

`MapRefreshValidation v0` binds the Map descriptor lifecycle to published
statistics evidence. It makes staleness, summarizability, and durable
publication checks explicit before a Map or its statistics can affect Procedure
planning. The contract is fail closed: analytical evidence can guide planning,
but it is not source of truth and cannot make data visible before durable
publication.

## Scope

This specification applies to validation performed before a Map, Map-derived
statistic, or correlation publication is eligible for optimizer use.

It covers:

- Map refresh mode validation for `Immediate`, `Incremental`, `Deferred`, and
  `SnapshotOnly`;
- Map staleness validation against source LSN, snapshot, mutation, age, and
  policy boundaries;
- summarizability validation for analytical Maps and rollups;
- current catalog stats checks for `StatsVersion`, `CatalogVersion`, histogram,
  NDV, skew, and correlation evidence;
- durable publication requirements before an active Map or stats pointer can be
  consumed;
- advisory-only use of analytics by the optimizer and plan cache;
- validation diagnostics and test coverage expectations.

## Current Implementation Status

The repository currently has `MapDescriptor v0` and `StatsObject v0`
specifications, catalog support for `ObjectKind::Map`, `StatsVersion` identity,
statistics publication switching, histogram and correlation evidence, and
tests that keep advisory evidence from driving active statistics directly.

This document narrows the validation contract around the current catalog stats
surface. It does not claim that a full Map engine, refresh scheduler, durable
Map storage layer, or complete optimizer MapLookup path is already implemented.

## Non-goals

This specification does not:

- introduce application-facing ad hoc SQL, dynamic table names, dynamic
  predicates, shape-shifting returns, runtime JSON defaults, or gRPC;
- replace typed Procedure contracts, SRPL cardinality rules, transaction
  visibility, or security admission;
- make Maps, histograms, NDV, skew markers, correlation evidence, GPU output,
  benchmark output, RAM state, temp files, or learned components database
  truth;
- allow candidate Map or candidate stats state to become active without
  validation and durable publication;
- define the physical byte layout for Map storage, statistics storage, WAL
  records, or snapshot manifests;
- put analytical refresh, GPU work, or large Map maintenance into commit, WAL,
  rollback, recovery, MVCC short-visibility, catalog publication, or
  security-critical paths.

## Prerequisites

Reviewers must use these references before accepting Map analytics validation
changes:

- `documentations/specs/MapDescriptor_v0.md` for Map identity, grain,
  summarizability, staleness, publication, source-of-truth boundaries, and
  optimizer eligibility.
- `documentations/specs/StatsObject_v0.md` for `StatsVersion`, publication
  states, advisory optimizer evidence, stale statistics, and extraction gaps.
- `documentations/specs/PlanCacheKey_v0.md` for plan-cache identity over
  catalog, statistics, policy, and shape evidence.
- `documentations/specs/WalRecord_v0.md` for durable WAL evidence boundaries.
- `documentations/specs/RecoveryReport_v0.md` for replay and validation
  reporting.
- DEC-016 and DEC-039 for plan-cache and optimizer decision trace contracts.

## Procedure

### Validation inputs

Every Map analytics validation decision must receive typed evidence. Textual
names, dynamic predicates, runtime JSON defaults, and command text are not
validation inputs.

| Input | Required evidence |
| --- | --- |
| Map identity | `CatalogObjectId`, qualified name, object kind `Map`, `CatalogVersion`, `MapVersion`, and descriptor digest. |
| Refresh policy | Refresh mode, policy version, cost bounds, trigger or schedule, retry behavior, and fail-closed policy. |
| Source lineage | Source object ids, source catalog versions, snapshot id or source LSN range, delta range when incremental, and refresh job id when applicable. |
| Grain | Row meaning, grain keys, uniqueness rule, dimension set, time or snapshot scope, and duplicate policy. |
| Summarizability | Additive class, aggregate functions, dimension hierarchy, null and absent handling, rollup eligibility, and confidence class. |
| Staleness | Maximum age, maximum source LSN lag, maximum mutation count or percentage, snapshot boundary, and stale-use policy. |
| Current catalog stats | Published `StatsVersion`, stats `CatalogVersion`, histogram, NDV, skew, correlation digest, evidence bounds, and validation trace. |
| Publication | Candidate state, validation result, durable WAL evidence, active pointer transition, rejection reason, and publication `DecisionTrace`. |

### Refresh and staleness gate

Map refresh validation must first prove that the requested Map version is
eligible under its refresh policy.

The validator must reject the candidate when:

- no refresh mode is declared;
- `Immediate` refresh has unbounded commit work;
- source lineage lacks a snapshot, source LSN range, delta range, or refresh
  job identity required by the refresh mode;
- the Map exceeds its staleness boundary and stale use is not explicitly
  allowed by policy;
- stale use is allowed but the consuming `DecisionTrace` cannot record the
  stale boundary, reason, risk penalty, and fallback choice.

Staleness is evaluated against the visible `CatalogVersion`, published
`StatsVersion`, policy version, and declared source boundary. A candidate Map,
candidate stats object, or correlation publication with mismatched catalog or
statistics identity is stale for that validation request.

### Summarizability gate

Summarizability validation decides whether a Map aggregate can be reused,
rolled up, joined, or treated as optimizer evidence for a requested shape.

The validator must reject or mark the Map ineligible when:

- grain is missing or ambiguous;
- the requested result shape is at a different grain with no declared rollup
  path;
- additive, semi-additive, non-additive, or holistic aggregate rules are not
  declared;
- duplicate handling, null handling, absent handling, or empty-set behavior is
  unspecified;
- dimension hierarchy rules are incomplete;
- correlation evidence is missing for a dependency-sensitive rollup and the
  fallback policy does not allow uncertainty;
- current catalog stats are stale, rejected, superseded, or unpublished.

Correlation evidence can support summarizability analysis only as advisory
evidence. It must not prove source table truth, must not override SRPL
cardinality contracts, and must not make an unsafe rollup valid by itself.

### Current catalog stats gate

Current catalog stats are eligible for Map analytics validation only when all
of the following are true:

1. The stats object or correlation publication is bound to the visible
   `CatalogVersion`.
2. The stats object or correlation publication is bound to the published
   `StatsVersion` selected for the validation request.
3. The statistics publication state is `Published`; `Candidate`, `Validating`,
   `Rejected`, `Expired`, and `Superseded` evidence is not current.
4. Durable publication evidence exists for the active stats pointer.
5. The consumed statistics digest and validation trace are recorded.
6. The consuming `DecisionTrace` records whether stats were used, rejected,
   stale, partial, or missing.

If any requirement fails, the validator must fail closed for critical use. For
non-critical planning, it may fall back to bounded conservative estimates only
when policy explicitly allows that fallback and the decision trace records the
reason.

### Durable publication gate

Analytics are not truth before durable publication.

The active Map pointer or active stats pointer must not advance until:

- descriptor, refresh, staleness, summarizability, and statistics validation
  have completed successfully;
- required WAL evidence has been emitted through an explicit codec;
- durable WAL coverage has been confirmed;
- the active pointer transition is atomic and traceable;
- recovery can reject incomplete, conflicting, or non-durable publication
  evidence.

RAM state, temp files, GPU output, benchmark output, learned output, partial
delta logs, and candidate statistics are not durable publication evidence. They
may be validation inputs only when bounded, typed, CPU-verifiable or otherwise
validated, and recorded as advisory.

### Optimizer and plan-cache use

The optimizer may use a Map or Map-derived statistic only when the validation
result is eligible for the Procedure risk class.

The consuming plan must record:

- `CatalogVersion`, `StatsVersion`, and policy version;
- Map identity, Map version, and descriptor digest when a Map is used;
- stats digest or consumed stats identifiers;
- stale-use policy and staleness boundary;
- summarizability decision, including rejected rollup reasons;
- fallback reason when stats or Maps are missing, stale, rejected, or partial;
- plan-cache key identity that separates changed catalog, stats, policy, grain,
  or shape evidence;
- `DecisionTrace` id for Map use or rejection.

Map and statistics evidence must not override typed Procedure contracts,
contract hashes, SRPL cardinality, transaction visibility, security admission,
or durable source truth.

### Diagnostics

Diagnostics must be stable, typed, sanitized, and correlated with the validation
trace.

| Diagnostic code | Meaning |
| --- | --- |
| `MAP-REFRESH-POLICY-MISSING` | Map descriptor has no refresh mode. |
| `MAP-IMMEDIATE-COST-UNBOUNDED` | Immediate refresh would add unbounded commit work. |
| `MAP-STATS-STALE` | Current catalog stats do not match the required `CatalogVersion` or `StatsVersion`. |
| `MAP-SUMMARIZABILITY-UNPROVEN` | Grain, aggregate, hierarchy, duplicate, or null/absent rules do not prove safe reuse. |
| `MAP-STATS-NOT-PUBLISHED` | Stats evidence is candidate, validating, rejected, expired, superseded, or missing. |
| `MAP-STATS-NOT-DURABLE` | Active stats use was attempted without durable publication evidence. |
| `MAP-ANALYTICS-NOT-TRUTH` | Analytics evidence was treated as source truth or semantic authority. |
| `MAP-DECISIONTRACE-MISSING` | Consuming plan did not record use, rejection, staleness, fallback, or risk evidence. |

Diagnostic messages must not contain raw row values, secrets, credentials,
unbounded source text, host paths, or command text.

## Validation

Documentation acceptance checks:

- The spec defines Map refresh validation for `Immediate`, `Incremental`,
  `Deferred`, and `SnapshotOnly`.
- The spec requires staleness validation against visible catalog, published
  stats, policy, and source boundaries.
- The spec requires summarizability validation for grain, aggregate class,
  dimension hierarchy, duplicate policy, null and absent handling, and rollup
  eligibility.
- The spec states that current catalog stats are advisory and must be bound to
  the visible `CatalogVersion` and published `StatsVersion`.
- The spec states that analytics are not source of truth before durable
  publication.
- The spec requires fail-closed behavior for stale, unpublished, rejected, or
  non-durable Map and stats evidence.
- The spec requires `DecisionTrace` records for Map use, Map rejection, stale
  use, summarizability decisions, and fallback planning.
- The spec does not introduce ad hoc SQL, dynamic table names, dynamic
  predicates, gRPC, runtime JSON defaults, or native Rust layout serialization.

Targeted validation:

```powershell
cargo test -p andromeda-catalog --test decision_coverage_invariants
cargo test -p andromeda-catalog --test statistics_builder_tests
cargo test -p andromeda-catalog --test stats_publication_switch_tests
```

Broader validation before claiming C5 readiness must include catalog
publication, WAL durability, recovery replay, and optimizer plan-cache gates.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Optimizer uses a stale Map silently. | Staleness was not compared with catalog, stats, policy, or source boundaries. | Reject the Map or record explicit stale-use policy and fallback in `DecisionTrace`. |
| Rollup double-counts source facts. | Summarizability validation missed grain, hierarchy, or duplicate policy. | Reject with `MAP-SUMMARIZABILITY-UNPROVEN` and require explicit rollup evidence. |
| Correlation stats validate a Map at the wrong version. | Stats evidence was not bound to the requested `CatalogVersion` and `StatsVersion`. | Treat the stats as stale and fail closed for critical use. |
| Active stats pointer advances from candidate analytics. | Durable publication gate was bypassed. | Reject with `MAP-STATS-NOT-DURABLE` until validation and durable WAL evidence are complete. |
| GPU or learned output becomes visible directly. | Advisory accelerator output bypassed CPU-verifiable validation and publication. | Keep it as candidate advisory evidence and require durable publication before active use. |
| Plan cache reuses a plan after Map or stats policy changes. | Plan-cache identity omitted catalog, stats, policy, grain, or shape evidence. | Build a new plan-cache key and trace invalidation. |
| Audit cannot explain Map use. | `DecisionTrace` did not record use, rejection, stale use, summarizability, or fallback reasons. | Reject the validation result or add a complete sanitized decision trace. |

## References

- `documentations/specs/MapDescriptor_v0.md`
- `documentations/specs/StatsObject_v0.md`
- `documentations/specs/PlanCacheKey_v0.md`
- `documentations/specs/WalRecord_v0.md`
- `documentations/specs/RecoveryReport_v0.md`
- `documentations/governance/decisions/DEC-016-plan-cache-scope.md`
- `documentations/governance/decisions/DEC-039-optimizer-intermediate-pass-contract.md`
- `crates/andromeda-catalog/src/statistics/correlation_publication.rs`
- `crates/andromeda-catalog/src/statistics/publication.rs`
