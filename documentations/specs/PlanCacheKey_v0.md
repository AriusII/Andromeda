# PlanCacheKey v0 Specification

## Purpose

Define the accepted documentation contract for `PlanCacheKey v0`, the bounded
identity used to separate, reuse, trace, and invalidate Procedure plans.

`PlanCacheKey v0` must be bounded, versioned, explainable, and invalidatable.
It is not an executable plan, not a cost model, not a benchmark result, and not
permission to bypass typed Procedure contracts.

## Scope

This specification applies to the optimizer, SRPL compiler, catalog plan-cache
gate, Procedure Store runtime evidence, and any future dedicated optimizer or
plan-cache crate that derives or consumes plan reuse identity.

It covers:

- the complete `PlanCacheKey v0` identity tuple;
- `PlanClass` and shape-fingerprint boundaries;
- construction and rejection rules;
- cache capacity and candidate-selection bounds;
- `DecisionTrace` evidence for plan selection and cache operations;
- invalidation rules for contract, catalog, statistics, policy, class, and
  shape changes;
- advisory-only ScenarioEvidence and Procedure feedback use;
- fallback behavior when plan reuse is disabled, stale, or invalid.

## Current Implementation Status

The repository already contains a catalog-owned plan-cache gate with
`PlanClass`, `CardinalityBucket`, `PlanShapeFingerprint`,
`PlanShapeFingerprintBuilder`, `PlanCacheKey`, bounded candidate selection,
advisory evidence summaries, closed cache-miss reasons, and a bounded
in-memory cache gate.

This specification is the target documentation contract for those concepts. It
does not claim that a full cost-based optimizer, dedicated optimizer crate, or
complete runtime plan cache policy is finished.

## Non-goals

This specification does not:

- define SRPL syntax, parsing, binding, or lowering;
- define the physical plan operator byte format;
- define a complete cost model;
- introduce application-facing ad hoc SQL, SQL-like command text, dynamic table
  names, dynamic predicates, gRPC, runtime JSON defaults, or untyped optimizer
  payloads;
- allow benchmark output, GPU output, learned model output, RAM state, or temp
  files to force a plan choice;
- allow statistics to override SRPL cardinality contracts, typed Procedure
  input or output shapes, permission checks, isolation policy, or WAL
  durability;
- serialize Rust native structs directly to disk or network;
- add new `PlanCacheKey` fields or `PlanClass` variants without a separate
  doctrine and compatibility decision.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- DEC-016 for the seven-field plan-cache identity and required invalidation
  inputs.
- DEC-039 for optimizer pass ordering, advisory cost estimates, and required
  plan-cache traces.
- `documentations/specs/ProcedureContract_v0.md` for typed Procedure contracts
  and contract hash binding.
- `documentations/specs/CatalogObjectModel_v0.md` for catalog publication and
  object versioning.
- `documentations/specs/StatsObject_v0.md` for `StatsVersion` publication and
  advisory optimizer evidence.
- `documentations/specs/AuditLedger_v0.md` for forensic trace boundaries.
- `crates/andromeda-catalog/src/plan_cache/` for the current Rust scaffold and
  bounded cache gate.

## Procedure

### Identity tuple

`PlanCacheKey v0` is exactly this identity tuple:

| Field | Rule | Invalidation impact |
| --- | --- | --- |
| `ProcedureId` | Nonzero identity of the cataloged Procedure. | A different Procedure must never reuse the entry. |
| `ContractHash` | Nonzero deterministic hash of the typed Procedure contract. | Any contract-affecting change separates the entry. |
| `CatalogVersion` | Nonzero catalog publication version used for binding. | Any catalog publication that affects planning separates or invalidates the entry. |
| `StatsVersion` | Nonzero published statistics version consumed by planning. | Any stats publication change separates or invalidates the entry. |
| `PolicyVersion` | Nonzero policy identity for permissions and resource policy. | Any relevant policy change separates or invalidates the entry. |
| `PlanClass` | Closed bounded class that explains why multiple plans may exist. | A different class must never alias the same cache slot. |
| `PlanShapeFingerprint` | Bounded fingerprint over typed shape and cardinality evidence. | A different shape fingerprint separates the entry. |

No other field may participate in `PlanCacheKey v0` identity. If future work
requires another axis, it must create a new specification version and a
doctrine decision instead of extending this tuple silently.

### Versioning

`PlanCacheKey v0` is versioned at three levels:

| Version axis | Requirement |
| --- | --- |
| Specification version | This document defines `v0`. A breaking identity change requires a new spec version. |
| Digest domain | The key digest must use a stable domain such as `andromeda.plan_cache.key.v0`. |
| Identity fields | `CatalogVersion`, `StatsVersion`, and `PolicyVersion` are part of equality, hashing, and trace evidence. |

Stable tags and domain strings must not be reordered or reused. Adding a new
`PlanClass` tag, changing cardinality buckets, or changing digest input order is
a compatibility event.

### Bounded PlanClass taxonomy

`PlanClass` is a closed taxonomy. The accepted `v0` classes are:

| PlanClass | Meaning | Shape-fingerprint rule |
| --- | --- | --- |
| `Singleton` | One plan per Procedure, contract, catalog, stats, and policy tuple. | Must use the empty shape fingerprint. |
| `ParameterShape` | Plans may differ by typed parameter shape. | Must use a non-empty fingerprint. |
| `Cardinality` | Plans may differ by bounded input cardinality bucket. | Must use a non-empty fingerprint derived from cardinality evidence. |
| `StatsAdaptive` | Plans may differ by combined shape and cardinality evidence. | Must use a non-empty fingerprint. |

The taxonomy prevents unbounded plan-class growth. A runtime must reject any
unknown class and must not accept free-form plan-class strings.

### Shape fingerprint

`PlanShapeFingerprint` is bounded evidence. It may include only typed inputs
that the builder accepts, such as:

- stable type tags;
- nullability or absence-policy evidence;
- structured-object arity;
- deterministic parameter slot order;
- bounded `CardinalityBucket` values.

The fingerprint must not include:

- raw parameter values;
- SQL text or SRPL source text;
- runtime memory addresses;
- debug formatting;
- timestamps, random values, or host paths;
- histograms, benchmark output, GPU output, learned model output, or temp
  storage state.

Cardinality evidence must be bucketed before it enters the fingerprint. Exact
runtime row counts must not create unbounded plan keys.

### Construction rules

A caller must build the key from a validated Procedure contract binding plus a
declared `PlanClass` and `PlanShapeFingerprint`.

Construction must fail closed when:

- `ProcedureId` is zero;
- `CatalogVersion` is zero;
- `ContractHash` is zero;
- `StatsVersion` is zero;
- `PolicyVersion` is zero;
- `PlanClass::Singleton` is paired with non-empty shape evidence;
- any non-singleton `PlanClass` is paired with the empty shape fingerprint.

A rejected key must produce a bounded diagnostic or trace reason. It must not
fall back to a partially bound key.

### Cache and selection bounds

Plan selection and cache admission must be bounded before reuse is enabled.

| Bound | Requirement |
| --- | --- |
| Candidate count | Selection must reject more candidates than the configured maximum. The current gate uses 8. |
| Advisory evidence count | A selection decision must reject excess ScenarioEvidence records. The current gate uses 8. |
| Cache capacity | Runtime cache capacity must be nonzero and capped. The current gate uses 64 entries as the maximum. |
| Eviction | Eviction must be deterministic and traceable. The current gate evicts the oldest entry when full. |
| Entry match | Cache hits require exact `PlanCacheKey` equality and matching key digest. |
| Plan digest | Stored entries must carry a nonzero plan digest supplied by the compiler or optimizer layer. |

The catalog cache gate does not own executable plan state. It stores opaque plan
identity evidence and must not store SRPL source text, SQL text, runtime JSON,
or executable payloads.

### Explainability

Every material plan decision must be explainable with `DecisionTrace` evidence.
The trace must record, at minimum:

- trace id;
- outcome: selected, hit, miss, insert, evict, invalidate, reject,
  stats-drift warning, or cost-accuracy alert;
- reason code from a closed set;
- key digest;
- `ProcedureId`;
- `ContractHash` digest or privacy-safe prefix;
- `CatalogVersion`;
- `StatsVersion`;
- `PolicyVersion`;
- `PlanClass`;
- shape-fingerprint digest;
- selected plan id when one exists;
- candidate count and matching candidate count;
- advisory evidence supplied, accepted, rejected, and ignored;
- cache-miss reason for misses;
- explicit `advisory_only=true` when ScenarioEvidence or Procedure feedback is
  summarized.

Trace records must avoid raw secrets, raw Procedure source, SQL text, raw
parameter values, and unbounded optimizer payloads.

### Invalidation lifecycle

Changing any identity field invalidates or separates the cache entry:

| Change | Required behavior |
| --- | --- |
| Procedure altered or dropped | New invocations must bind a new `ContractHash` or fail before transaction creation. Existing cache slots must not be reused for the new binding. |
| Catalog publication advances | A plan bound to the old `CatalogVersion` must miss or be evicted before reuse under the new version. |
| Statistics publication advances | A plan bound to the old `StatsVersion` must miss or be evicted before reuse under the new version. |
| Policy publication advances | A plan bound to the old `PolicyVersion` must miss or be evicted before reuse under the new policy. |
| PlanClass changes | The new class must use a separate key and trace why specialization changed. |
| Shape fingerprint changes | The new shape must use a separate key. `Singleton` cannot accept shape evidence. |
| Map descriptor or Map stats change affects planning | The change must advance `CatalogVersion`, `StatsVersion`, or `PolicyVersion` so plan reuse separates through the key. |

Invalidation must be observable. Silent reuse across changed identity evidence is
a spec violation.

### Disablement and fallback

Plan-cache reuse must be disableable by policy or operator configuration. When
reuse is disabled, the runtime may still derive a key for trace correlation, but
it must not treat a cache entry as authoritative.

Fallback behavior must:

1. bind the Procedure contract normally;
2. select or compile a plan through the bounded optimizer path;
3. emit a trace that names the disablement or miss reason;
4. preserve the same transaction, WAL, catalog, policy, and security gates.

Disabling the cache must not weaken Procedure contract validation.

### Hysteresis and advisory evidence

Procedure Store feedback, ScenarioEvidence, statistics drift, and cost-accuracy
alerts are advisory inputs. They may increase risk penalty, explain why a plan
class was selected, or recommend invalidation, but they must not become the sole
authority that forces a plan.

Any anti-flapping or hysteresis policy must be:

- bounded by a fixed threshold or explicit policy;
- versioned by `PolicyVersion` when it affects reuse;
- observable in `DecisionTrace`;
- disableable;
- unable to override SRPL semantics, typed contracts, permissions, or durable
  publication.

### Durable and recovery boundaries

`PlanCacheKey v0` is identity evidence, not database truth. An in-memory cache
entry may be discarded at any time. Recovery must be able to rebuild valid plan
state from cataloged Procedure contracts, published catalog state, published
statistics, policy state, and durable Procedure or audit evidence.

If a future persisted plan-cache artifact is introduced, it must use an explicit
canonical codec and recovery validation. Rust native struct layout must never be
the durable or network contract.

## Validation

Documentation acceptance checks:

- The spec defines exactly seven key fields.
- The spec states that all identity fields participate in equality, hashing,
  trace evidence, and invalidation.
- The spec defines closed `PlanClass` values and their shape-fingerprint rules.
- The spec states that candidate selection, advisory evidence, and cache
  capacity are bounded.
- The spec requires `DecisionTrace` evidence for selection, hit, miss,
  insertion, eviction, invalidation, rejection, stats drift, and cost-accuracy
  alerts.
- The spec states that ScenarioEvidence, Procedure feedback, benchmark output,
  GPU output, and learned output are advisory only.
- The spec states that a changed `ContractHash`, `CatalogVersion`,
  `StatsVersion`, `PolicyVersion`, `PlanClass`, or `PlanShapeFingerprint`
  prevents silent reuse.
- The spec does not introduce ad hoc SQL, dynamic table names, dynamic
  predicates, gRPC, runtime JSON defaults, or native Rust layout serialization.

Future implementation work should add or keep targeted validation for:

```powershell
cargo test -p andromeda-catalog --quiet plan_cache
cargo test -p andromeda-catalog --test plan_invalidation
cargo test -p andromeda-srpl --quiet optimizer
cargo clippy -p andromeda-catalog -- -D warnings
```

These commands are not required for documentation-only acceptance.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| A plan is reused after a `StatsVersion` change. | Cache lookup ignored the complete key or accepted a partial key. | Reject reuse unless exact `PlanCacheKey` equality and digest match. |
| A singleton plan has a shape fingerprint. | The caller specialized a `Singleton` plan. | Reject key construction and use a shaped `PlanClass` if policy allows it. |
| Cache size grows without a cap. | Runtime admission skipped capacity policy. | Enforce a nonzero bounded capacity and deterministic eviction. |
| A trace cannot explain a miss. | Miss reason was not classified. | Emit a closed cache-miss reason such as stats, catalog, policy, class, or shape mismatch. |
| Benchmark evidence selects a plan by itself. | Advisory evidence was treated as authority. | Restore static bounded selection and record benchmark evidence as advisory only. |
| A policy-disabled cache still returns hits. | Disablement was applied after lookup. | Gate reuse before lookup or treat lookup as trace-only evidence. |
| A Map refresh changes plan quality without invalidation. | Map publication did not advance catalog, stats, or policy identity. | Publish the relevant version change and force a new key. |

## References

- `documentations/governance/decisions/DEC-016-plan-cache-scope.md`
- `documentations/governance/decisions/DEC-039-optimizer-intermediate-pass-contract.md`
- `documentations/specs/ProcedureContract_v0.md`
- `documentations/specs/CatalogObjectModel_v0.md`
- `documentations/specs/StatsObject_v0.md`
- `documentations/specs/AuditLedger_v0.md`
- `documentations/05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md`
- `crates/andromeda-catalog/src/plan_cache/identity.rs`
- `crates/andromeda-catalog/src/plan_cache/selection.rs`
- `crates/andromeda-catalog/src/plan_cache/admission.rs`
