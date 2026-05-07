# DEC-016: V0.5 Runtime Plan Cache Scope

## Status

Accepted.

## Context

E5 asks whether V0.5 should add a runtime plan cache or keep the existing
`andromeda-catalog` plan-cache module as a scaffold.  The current catalog code
already defines bounded `PlanClass` variants, deterministic
`PlanShapeFingerprint` construction, and `PlanCacheKey` identity over
`ProcedureId`, `ContractHash`, `CatalogVersion`, `StatsVersion`,
`PolicyVersion`, plan class, and shape evidence.

A runtime cache would turn those identities into executable reuse behavior.  That
requires settled publication and invalidation semantics for catalog lifecycle,
statistics versions, policy versions, and optimizer decision traces.  Those
runtime semantics are not yet implemented in V0.5.

## Decision

V0.5 does **not** add a runtime plan cache.  The plan-cache surface remains a
scaffold that defines and validates cache-key identity only.

The scaffold is strengthened so `PlanCacheKey::build` rejects unbound identity
inputs before a key can be used:

- zero `ProcedureId`;
- zero `CatalogVersion`;
- zero `ContractHash`;
- zero `StatsVersion`;
- zero `PolicyVersion`;
- `PlanClass::Singleton` with non-empty shape evidence;
- any shaped `PlanClass` with empty shape evidence.

This preserves the contract-before-transaction doctrine: a future runtime cannot
derive a reusable plan key from an unbound or mismatched procedure contract.

## Required Invalidation Inputs

Any future runtime cache entry must be invalidated or separated by the complete
`PlanCacheKey` identity:

1. `ProcedureId`;
2. `ContractHash`;
3. `CatalogVersion`;
4. `StatsVersion`;
5. `PolicyVersion`;
6. `PlanClass`;
7. `PlanShapeFingerprint`.

Changing any one of these inputs must produce a different key digest and must not
reuse an existing cached plan slot.

## DecisionTrace Requirements

When a runtime cache is added, every plan-cache hit, miss, insertion, eviction,
or invalidation must be traceable.  The trace must record enough deterministic
evidence to reproduce why reuse was or was not allowed:

- plan-cache key digest;
- procedure id;
- catalog version;
- stats version;
- policy version;
- contract hash or a privacy-preserving contract-hash digest already present in
  the key;
- plan class;
- shape-fingerprint digest;
- scenario/procedure-feedback/statistics evidence identifiers consumed by the
  optimizer, when any;
- decision outcome (`hit`, `miss`, `insert`, `evict`, `invalidate`, or
  `reject`);
- bounded rejection reason for invalid keys or expired evidence.

Trace records must not introduce SQL text, runtime JSON defaults, gRPC, or
free-form optimizer payloads.

## Capacity and Eviction

No V0.5 runtime cache exists, so there is no active capacity or eviction policy.

A future implementation must define both before enabling reuse.  The minimum
acceptable policy is:

- hard non-zero capacity configured at construction;
- deterministic bounded eviction;
- no unbounded per-procedure or global growth;
- eviction traces containing the evicted key digest and cause;
- no reuse across changed catalog, stats, policy, contract, plan-class, or shape
  evidence inputs.

## Invariants Preserved

- Procedure contracts remain typed, versioned, and hashed.
- Contract/binding mismatches are rejected before transaction creation.
- Plan decisions remain traceable by requirement and are not silently made by
  catalog scaffolding.
- No ad hoc SQL, SQL-like runtime surface, gRPC, or runtime JSON default is
  introduced.
- Protobuf remains a boundary format only.
- The catalog crate remains unsafe-free and within existing crate boundaries.

## Validation

- `cargo test -p andromeda-catalog --quiet`
- `cargo test -p andromeda-catalog --quiet plan_cache`
