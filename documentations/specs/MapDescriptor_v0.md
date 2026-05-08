# MapDescriptor v0 Specification

## Purpose

Define the accepted documentation contract for `MapDescriptor v0`, the catalog
descriptor for materialized Maps.

A Map is a materialized projection with a declared definition, grain, storage,
statistics, lineage, and consistency policy. Source tables, published
snapshots, and durable WAL remain the source of truth. Maps are not source of
truth before durable publication, and candidate Map state, delta logs, GPU
output, RAM state, temp files, and benchmark output are never database truth by
themselves.

## Scope

This specification applies to cataloged Maps, Map dependencies, Map refresh
policies, Map staleness decisions, Map publication, Map optimizer use, and
future dedicated Map runtime or storage modules.

It covers:

- Map identity and versioning;
- canonical definition IR evidence;
- declared grain;
- summarizability constraints;
- refresh policy: `Immediate`, `Incremental`, `Deferred`, and `SnapshotOnly`;
- staleness policy and optimizer eligibility;
- lineage from source objects, snapshots, WAL, delta ranges, and refresh jobs;
- validation and durable publication;
- storage layout and statistics links;
- resource and GPU boundaries;
- troubleshooting and validation criteria.

## Current Implementation Status

The repository already models `ObjectKind::Map`, has catalog Protobuf messages
for immediate and incremental Maps, documents Map refresh modes in the roadmap,
and includes `MapDeltaAppend` as WAL and recovery classification evidence.

A dedicated Map engine crate is not yet established. This specification defines
the target descriptor contract without claiming that all runtime maintenance,
storage, optimizer, or recovery behavior is complete.

## Non-goals

This specification does not:

- introduce generic virtual views as a native runtime surface;
- introduce application-facing ad hoc SQL, dynamic table names, dynamic
  predicates, shape-shifting returns, gRPC, runtime JSON defaults, or untyped
  Map payloads;
- define every physical row, column, segment, compression, or dictionary byte
  format;
- define every SRPL syntax rule for `map` declarations;
- replace typed Procedure contracts or Procedure result cardinality;
- make a Map the authoritative source for source table truth;
- allow a large analytical Map to run on the OLTP commit path without strict
  bounds;
- allow GPU work in commit, WAL, rollback, recovery, MVCC short-visibility,
  catalog publication, or security-critical paths;
- allow security Maps to replace SecurityAdmission or IAM policy evaluation;
- serialize Rust native structs directly to disk or network.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `documentations/02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md` for Map role,
  refresh modes, and SRPL aggregate behavior.
- `documentations/05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md` for
  analytics, summarizability, `MapLookup`, and GPU boundaries.
- `documentations/specs/CatalogObjectModel_v0.md` for catalog identity,
  dependency graph, publication, WAL, and recovery evidence.
- `documentations/specs/StatsObject_v0.md` for Map statistics and
  `StatsVersion` publication.
- `documentations/specs/WalRecord_v0.md` for `MapDeltaAppend` and WAL record
  boundaries.
- `documentations/specs/RecoveryReport_v0.md` for replay and validation
  reporting.
- `documentations/specs/SecurityAdmission_v0.md` for security and permission
  admission boundaries.
- DEC-030 for DefinitionBatch dependency validation involving Maps.

## Procedure

### Descriptor model

`MapDescriptor v0` must be a bounded, typed catalog object. Documentation may
adapt field names to local implementation naming, but it must preserve the
following semantics:

| Field group | Required evidence |
| --- | --- |
| Identity | `MapId` or `CatalogObjectId`, object kind `Map`, qualified name, namespace, `CatalogVersion`, `MapVersion`, descriptor schema version. |
| Definition | Canonical typed definition IR digest, source Procedure or SRPL definition binding, result columns, output StructuredObject contracts when applicable. |
| Grain | Grain key, row meaning, uniqueness rule, dimension set, time bucket or snapshot scope when applicable. |
| Summarizability | Aggregate functions, additive class, dimension hierarchy rules, empty-set behavior, duplicate policy, null and absent handling. |
| Refresh policy | Mode, trigger, maximum apply cost, maximum commit cost if immediate, schedule or job class, retry and fail-closed behavior. |
| Staleness | Maximum age, maximum source LSN lag, maximum mutation count or percentage, snapshot id, stale-use policy, optimizer eligibility. |
| Lineage | Source object ids and versions, dependency edges, source snapshot or WAL range, delta range, refresh job id, validation trace id. |
| Storage | Backing table, column segments, row or column layout, compression, access paths, segment manifest, cold snapshot link when applicable. |
| Statistics | `StatsVersion`, row count basis, histograms, NDV, skew, compression stats, publication state. |
| Publication | Candidate state, validation state, durable WAL evidence, published pointer, superseded version, rejection reason. |
| Security | Required permission families, policy version, SecurityAdmission link for security-sensitive Maps. |
| Observability | Refresh trace, publication trace, validation diagnostics, consuming optimizer `DecisionTrace` ids. |

The descriptor must not store raw secrets, raw credentials, unbounded source
text, host-specific paths, or free-form command text.

### Identity and versioning

Map identity is durable and versioned.

| Field | Rule |
| --- | --- |
| `MapId` or `CatalogObjectId` | Stable nonzero identity for the Map object. |
| `MapVersion` | Monotonic version of the Map descriptor or materialized state within the Map object. |
| `CatalogVersion` | Catalog publication version that makes the descriptor visible. |
| `MapDefinitionHash` | Deterministic digest over canonical definition IR and result shape. |
| `StatsVersion` | Published statistics version for the Map when optimizer evidence exists. |
| `PolicyVersion` | Policy version that controls refresh, staleness, permissions, and resource budgets. |

A descriptor-affecting change must create a new catalog object version or a new
`MapVersion`. A statistics-only change must advance `StatsVersion`. A refresh or
staleness-policy change that affects optimizer eligibility must advance
`PolicyVersion` or the catalog version that binds it.

### Definition IR and dependencies

The Map definition must be represented by canonical typed IR or a canonical
digest over that IR. The catalog may store opaque definition bytes only when the
SRPL layer owns validation and the bytes are bounded by an explicit codec.

Dependencies must use durable object identity:

- source table object ids and visible version ranges;
- source Map ids when composing Maps;
- Procedure or type descriptor ids when the definition depends on them;
- policy ids and versions;
- statistics ids or required `StatsVersion` values when planning depends on
  them.

Names may be used for diagnostics. Names must not replace object ids and version
ranges as dependency truth.

### Grain

Every Map must declare its grain. Grain states what one Map row represents.

The descriptor must define:

- grain key fields;
- source entity or relationship represented by one row;
- whether the grain is per source row, per source key, per join key, per time
  bucket, per aggregate group, per tenant, or per snapshot;
- uniqueness and duplicate policy;
- dimensional attributes that are part of the grain;
- stable ordering policy only when order is contractually required;
- whether the Map is exact for the grain or approximate advisory evidence.

Grain must not be inferred from selected columns, index shape, or sample data.
If the grain is ambiguous, validation must reject the descriptor.

### Summarizability

Summarizability defines whether Map aggregates can be rolled up, joined, or
reused without changing meaning.

`MapDescriptor v0` must distinguish:

| Aggregate class | Rule |
| --- | --- |
| Additive | May roll up across declared compatible dimensions when grain and duplicate policy allow it. |
| Semi-additive | May roll up only across specified dimensions. Time and snapshot semantics must be explicit. |
| Non-additive | Must not be rolled up without a declared recomputation rule. |
| Distinct or approximate | Must preserve algorithm, precision, confidence, and mergeability rules. |
| Empty-set aggregate | Must use explicit SRPL behavior, such as `0`, `absent`, or a declared failure. |

Summarizability validation must check:

- declared grain is stable;
- dimension hierarchies do not double-count;
- joins do not multiply facts unless the descriptor declares and bounds the
  duplicate policy;
- null, absent, unknown, and not-collected states remain distinct;
- aggregate functions are compatible with the requested reuse;
- security and tenant boundaries are not crossed by rollup.

### Refresh policy

Each Map must declare exactly one refresh mode.

| Mode | Semantics | Required evidence |
| --- | --- | --- |
| `Immediate` | Map update is part of the same transaction as the source mutation. | Source and Map WAL coverage, bounded commit cost, same transaction id, atomic visibility. |
| `Incremental` | Source mutations append durable delta evidence, and a controlled apply step refreshes Map state. | Delta WAL range, apply job id, validation trace, publication trace. |
| `Deferred` | Refresh runs outside the user transaction by an administration or maintenance job. | Schedule or trigger, source snapshot or LSN range, resource budget, publication trace. |
| `SnapshotOnly` | Map is consistent only with a published snapshot. | Snapshot id, manifest, source LSN boundary, immutable publication evidence. |

V0 policy defaults:

- simple projection Maps may use `Immediate` only when commit cost is bounded;
- aggregate Maps should use `Incremental` unless the bounded immediate cost is
  explicitly proven;
- large analytical Maps should use `Deferred` or `SnapshotOnly`;
- a Map used by a critical Procedure requires explicit staleness and optimizer
  eligibility policy.

### Staleness policy

Staleness is first-class descriptor evidence. It must not be hidden in
implementation notes.

The descriptor must define at least one staleness boundary:

- maximum age;
- maximum source LSN lag;
- maximum un-applied delta count;
- maximum source mutation percentage;
- snapshot id or snapshot generation;
- explicit `fresh_only`, `stale_allowed`, or `never_for_critical_plan` policy.

Map use must fail closed when staleness exceeds policy. If stale use is allowed,
the consuming `DecisionTrace` must record the stale boundary, reason, and risk
penalty.

### Lineage

Lineage must make the published Map reconstructable and auditable.

Required lineage evidence includes:

- descriptor version and definition hash;
- source object ids, object versions, and dependency edge kinds;
- source snapshot id or source WAL LSN range;
- delta log range for incremental refresh;
- refresh job id or Procedure invocation id that produced the candidate;
- validation trace id;
- publication trace id;
- published `StatsVersion`;
- superseded version when applicable;
- rejection or quarantine reason when validation fails.

Lineage must be immutable after publication. A correction requires a new
candidate and a new publication decision.

### Validation and publication lifecycle

Map publication must use explicit states.

| State | Meaning | Visibility |
| --- | --- | --- |
| `Candidate` | Descriptor or materialized state has been built but not validated. | Not visible for Procedure planning or execution. |
| `Validating` | Consistency, grain, summarizability, staleness, lineage, and policy checks are running. | Not visible except isolated validation. |
| `WalRecorded` | Required Map descriptor, delta, or publication WAL evidence exists. | Not visible until durable WAL coverage is confirmed. |
| `Published` | Durable publication succeeded and the active pointer advanced. | Visible according to refresh and staleness policy. |
| `Rejected` | Validation, policy, dependency, or durability checks failed. | Never visible for new use. |
| `Expired` | Published version exceeded staleness policy. | Not eligible unless a policy explicitly allows stale advisory use. |
| `Superseded` | A newer published version replaced this version. | Historical lookup only. |

Publication sequence:

1. Read the current catalog version and active source dependencies.
2. Build the candidate descriptor or candidate materialized state.
3. Validate dependencies, grain, summarizability, refresh policy, staleness,
   resource budget, security policy, and lineage.
4. Emit deterministic WAL evidence for descriptor publication, immediate Map
   mutation, or incremental delta publication as applicable.
5. Confirm durable WAL coverage.
6. Atomically publish the Map version or descriptor version.
7. Publish or bind the associated `StatsVersion` when statistics are available.
8. Emit refresh, validation, publication, and recovery evidence.

No caller may observe a candidate, validating, or WAL-recorded but not published
Map as active.

### Source-of-truth boundary

Maps are derived state. The authoritative truth remains the latest valid source
snapshot plus durable WAL from that snapshot, governed by catalog publication
and transaction rules.

The following are not source of truth:

- candidate Map state;
- unvalidated Map state;
- Map delta logs before controlled apply and publication;
- RAM caches;
- temp files;
- GPU results;
- benchmark results;
- learned-model predictions;
- stale Maps outside policy.

A published Map may be used as query input or optimizer evidence only at its
declared consistency boundary. It must not rewrite source table truth, bypass
WAL, or make a commit visible before durable publication.

### Optimizer and PlanCache interaction

The optimizer may use a Map through a `MapLookup` or equivalent physical
operator only when:

- the Procedure contract allows the result shape;
- the Map descriptor is visible at the selected `CatalogVersion`;
- the Map `StatsVersion` used for costing is published and traced;
- the staleness policy allows use for the Procedure risk class;
- grain and summarizability match the requested result;
- permissions and policy gates pass;
- the plan records a `DecisionTrace` explaining Map use or rejection.

Map use must not:

- change SRPL cardinality contracts;
- change typed Procedure input or output shapes;
- bypass SecurityAdmission;
- reuse a plan across changed catalog, statistics, policy, grain, or staleness
  evidence;
- treat an analytical Map as fresher than its declared snapshot or LSN range.

Map descriptor, Map statistics, or Map policy changes that affect planning must
advance `CatalogVersion`, `StatsVersion`, or `PolicyVersion` so
`PlanCacheKey v0` separates or invalidates dependent plans.

### Storage layout and statistics

`MapDescriptor v0` may declare one of these layout classes:

| Layout | Use |
| --- | --- |
| `Row` | Small or transactional Maps with point lookup or narrow projection needs. |
| `Column` | Analytical Maps, scans, aggregates, compression, and snapshot reporting. |
| `Hybrid` | Mixed lookup and analytical workloads when the descriptor declares which columns or segments use each layout. |

The descriptor must record the backing object or segment identity, not raw
native struct layout. Durable and network formats require explicit codecs.

Statistics for Maps must follow `StatsObject v0`: row count, histograms, NDV,
density, skew, compression stats, coverage, confidence, publication state, and
consuming `DecisionTrace` links.

### Resource and GPU boundaries

Map maintenance must not starve WAL, checkpoint, recovery, catalog publication,
security admission, or short OLTP visibility paths.

Required resource policy includes:

- maximum commit work for `Immediate` Maps;
- maximum delta batch size for `Incremental` Maps;
- maximum job duration, memory, temp bytes, and IO for `Deferred` and
  `SnapshotOnly` Maps;
- cancellation and retry policy;
- backpressure behavior when maintenance falls behind.

GPU acceleration is allowed only for optional batch analytics, statistics, or
Map refresh work outside commit, WAL, rollback, recovery, MVCC short-visibility,
catalog publication, and security-critical paths. GPU output must be
CPU-verifiable or otherwise validated before publication. A failed GPU job must
fall back to CPU or reject the candidate without corrupting source truth.

### Diagnostics

Map diagnostics must be stable, typed, sanitized, and correlated with the
descriptor, refresh job, or DefinitionBatch operation.

| Diagnostic code | Meaning |
| --- | --- |
| `MAP-GRAIN-MISSING` | Descriptor does not declare grain. |
| `MAP-GRAIN-AMBIGUOUS` | Grain does not uniquely describe a row. |
| `MAP-SUMMARIZABILITY-INVALID` | Aggregate, dimension, or duplicate policy cannot be safely rolled up. |
| `MAP-REFRESH-POLICY-MISSING` | Descriptor has no refresh mode. |
| `MAP-IMMEDIATE-COST-UNBOUNDED` | Immediate refresh would put unbounded work on the commit path. |
| `MAP-STALENESS-EXCEEDED` | Map version exceeds policy and is not eligible for use. |
| `MAP-LINEAGE-INCOMPLETE` | Required source snapshot, LSN, delta, or refresh job evidence is missing. |
| `MAP-WAL-NOT-DURABLE` | Publication was attempted before durable WAL coverage. |
| `MAP-SECURITY-POLICY-MISMATCH` | Security or tenant boundary does not match the requested use. |
| `MAP-PUBLICATION-REJECTED` | Candidate failed validation or policy gates. |

Diagnostic messages must not include raw secrets, credentials, unbounded source
text, or raw user data.

## Validation

Documentation acceptance checks:

- The spec defines Map identity, versioning, definition evidence, grain,
  summarizability, refresh policy, staleness, lineage, validation, publication,
  and optimizer interaction.
- The spec states that Maps are not source of truth before durable publication.
- The spec states that source tables, snapshots, and durable WAL remain the
  authoritative truth.
- The spec defines `Immediate`, `Incremental`, `Deferred`, and `SnapshotOnly`
  refresh modes.
- The spec requires explicit staleness boundaries and fail-closed behavior.
- The spec requires lineage through source objects, versions, snapshots or WAL
  ranges, delta ranges, refresh jobs, validation traces, publication traces, and
  `StatsVersion`.
- The spec requires `MapLookup` use to respect staleness, policy, grain,
  summarizability, permissions, and plan-cache invalidation.
- The spec keeps GPU work out of commit, WAL, rollback, recovery, MVCC
  short-visibility, catalog publication, and security-critical paths.
- The spec does not introduce ad hoc SQL, generic virtual views, dynamic table
  names, dynamic predicates, gRPC, runtime JSON defaults, or native Rust layout
  serialization.

Future implementation work should add targeted validation for:

```powershell
cargo test -p andromeda-catalog --test catalog_dependency_graph_contract
cargo test -p andromeda-catalog --test catalog_publication_subscription
cargo test -p andromeda-storage --test recovery_completeness_contract
cargo test -p andromeda-storage --test crash_recovery_impl
cargo test -p andromeda-catalog --quiet plan_cache
```

These commands are not required for documentation-only acceptance.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| A Map row cannot be interpreted. | Grain is missing or ambiguous. | Reject the descriptor with `MAP-GRAIN-MISSING` or `MAP-GRAIN-AMBIGUOUS`. |
| Aggregate rollup double-counts facts. | Summarizability rules ignored duplicate or dimension hierarchy policy. | Require explicit additive class, dimension hierarchy, and duplicate policy. |
| A large analytical Map slows OLTP commit. | Refresh mode was set to `Immediate` without bounded cost. | Move to `Incremental`, `Deferred`, or `SnapshotOnly`, or prove strict commit bounds. |
| Optimizer uses a stale Map silently. | Staleness policy was not checked or traced. | Fail closed or record stale-use policy and risk penalty in `DecisionTrace`. |
| Recovery cannot rebuild Map state. | Lineage lacks source LSN, snapshot, delta range, or publication trace. | Reject publication until lineage is complete and durable. |
| GPU refresh result becomes visible directly. | GPU output bypassed validation and durable publication. | Treat GPU output as candidate evidence only and require CPU-verifiable validation. |
| A security Map bypasses IAM. | Map evidence was treated as SecurityAdmission authority. | Route final authorization through SecurityAdmission and policy version checks. |
| Plan cache reuses a plan after Map policy changes. | Map change did not advance catalog, stats, or policy identity. | Publish the relevant version change and force a new `PlanCacheKey`. |

## References

- `documentations/02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md`
- `documentations/05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md`
- `documentations/ANDROMEDA_ROADMAP_IMPLEMENTATION_CROSSCHECK_2026.md`
- `documentations/specs/CatalogObjectModel_v0.md`
- `documentations/specs/StatsObject_v0.md`
- `documentations/specs/WalRecord_v0.md`
- `documentations/specs/RecoveryReport_v0.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `documentations/specs/PlanCacheKey_v0.md`
- `crates/andromeda-proto/proto/andromeda/contract/v1/catalog.proto`
- `crates/andromeda-storage/src/recovery/replay/deferred.rs`
