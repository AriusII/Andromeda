# DEC-039: implementation-to-release SRPL Optimizer Intermediate Pass Contract

## Status

PROPOSED — awaiting doctrine-guardian ratification before release gate cycle
optimizer-pass activation.

## Context

Batch 19 (implementation batch, work item `v1-srpl-constant-folding-s01-audit`) audited
the SRPL compiler pipeline and identified the absence of a structured `Expr`
enum as the root blocker for constant folding. The audit showed that
expressions in the AST are `Spanned<String>` and only partially lifted in
the IR layer: `SrplValueIr` carries `Bool(bool)` as its only typed constant,
and `SrplPredicateIr` carries no literal nodes at all.

Batch 20 (implementation batch, work item `v1-srpl-constant-folding-s02-contract`)
produced an optimizer contract specifying the folding semantics,
determinism constraints, and error-deferral rules. That document was
accepted as a design proposal but not yet as doctrine.

Batch 21 (implementation batch, work item `v1-srpl-constant-folding-s03-design`) extends
both prior deliveries into a full six-task optimizer architecture covering
constant folding, predicate pushdown, projection pushdown, plan caching,
cost modelling, and plan choice. It also delivers the first concrete Rust
implementations of every optimizer pass in `andromeda-srpl::optimizer`.

This decision record ratifies the combined design into a
doctrine-level commitment that governs all release gate cycle optimizer work.

## Decision

### 1. Typed Constant Representation

`ConstantLiteral` is introduced in `andromeda-srpl::ir` as the minimal
closed constant representation needed by the optimizer. Its variants are:

- `Bool(bool)`
- `Int64(i64)`
- `Uint64(u64)`
- `Decimal { integer_part: i128, scale: u8 }` (invariant: `scale <= 18`)

Adding a new variant is a doctrine change.

### 2. Extended `SrplValueIr`

Two new variants are added to `SrplValueIr`:

- `Constant(ConstantLiteral)` — a compile-time constant node.
- `BinaryArith { op: ArithOp, left: Box<SrplValueIr>, right: Box<SrplValueIr> }` —
  bounded binary arithmetic with maximum nesting depth `MAX_EXPR_DEPTH = 8`.

`ArithOp` has four variants: `Add`, `Subtract`, `Multiply`, `Divide`.

The existing `Bool(bool)` variant is deprecated in favour of
`Constant(ConstantLiteral::Bool(_))` but must not be removed until all
call-sites are migrated. These changes do not alter any wire-format boundary
because `SrplValueIr` is an internal IR type only.

### 3. Optimizer Pipeline Order

The optimizer pipeline is enumerated by `OptimizerPhase` with eight ordered
phases:

```
Parsing (0) → Binding (1) → IRLowering (2) → ConstantFolding (3)
→ PredicatePushdown (4) → ProjectionPushdown (5) → CostAnalysis (6)
→ PlanChoice (7)
```

No phase may be reordered at runtime. Adding a phase is a doctrine change.

### 4. Non-Deterministic Function Guard

Non-deterministic functions must **never** be constant-folded. The closed
registry `function_fold::classify_builtin` is the sole authority.

Functions that must never be folded (illustrative, not exhaustive):
`NOW`, `RAND`, `RANDOM`, `UUID`, `NEWID`, `GEN_RANDOM_UUID`,
`CURRENT_TIMESTAMP`, `TRANSACTION_TIMESTAMP`, `SYSDATE`, `GETDATE`.

Any function name not in the registry defaults to
`FunctionDeterminism::NonDeterministicPerInvocation`. This default is safe:
it prevents folding of unknown functions.

### 5. Division by Zero and Overflow Deferral

Division by zero and arithmetic overflow detected during constant folding
are **deferred to runtime**. The fold pass must not raise a compile-time
error. The unresolved `BinaryArith` node must remain in the IR. The
optimizer emits an advisory `DecisionTrace` record to note the deferral.

### 6. Predicate Pushdown Eligibility

Predicate pushdown is applied only to predicates that satisfy all six
eligibility rules E1–E6:

- **E1** Single binding reference.
- **E2** Scalar predicate (atomic comparison only).
- **E3** No correlation (predicate input does not name another Read's binding).
- **E4** Type compatibility between input and field.
- **E5** No intervening mutation on the same source between Read and Assert.
- **E6** Cardinality compatibility.

A predicate that fails any rule stays in place. No override path exists.

### 7. Projection Pushdown Liveness Invariant

Projection pushdown must not remove any column that the backward liveness
analysis marks as live. The `ColumnLiveness::compute` backward pass is the
sole authority on live columns. No column in any `USE` set may be pruned.

### 8. Cost Estimates Are Advisory

Cost estimates produced by `CostModel::estimate_without_stats` or any
future `CostModel::estimate` implementation are advisory. They must not
override SRPL cardinality contracts (`Cardinality::One`, `affected_rows_exact`,
`Cardinality::NonEmptyMany`). A high cost estimate does not permit altering
the declared cardinality.

### 9. Plan Cache Activation

release gate cycle runtime plan cache activation must use `PlanCacheKey::build` as
defined in DEC-016. No new `PlanCacheKey` fields may be added. No new
`PlanClass` variants may be added without a separate doctrine change.

### 10. Plan Cache Trace Requirement

Every plan cache operation (hit, miss, insert, evict, invalidate, reject,
stats-drift warning, cost-accuracy alert) must emit a `DecisionTrace` with
at minimum:

- key digest (32-byte SHA-256);
- procedure id;
- catalog version;
- stats version;
- policy version;
- plan class;
- shape fingerprint digest;
- decision outcome (closed enum).

No free-form payload, no SQL text, no gRPC surface.

## Required Invariants

INV-01 through INV-15 as documented in
`v1-srpl-constant-folding-s03-design` Section 3 are binding constraints
for all optimizer work in release gate cycle.

| ID | Invariant |
| --- | --- |
| INV-01 | No SQL or SQL-like surface |
| INV-02 | No gRPC in runtime path |
| INV-03 | No unsafe Rust in optimizer crates |
| INV-04 | Contracts typed, versioned, and hashed before any plan |
| INV-05 | Plan cache key includes all 7 identity fields |
| INV-06 | Every cache operation emits a DecisionTrace |
| INV-07 | Optimization passes preserve semantic equivalence |
| INV-08 | Non-deterministic functions never folded |
| INV-09 | Division-by-zero / overflow deferred to runtime |
| INV-10 | MAX_SRPL_BODY_OPERATIONS = 16 never exceeded |
| INV-11 | Ordinals dense and zero-based after any transformation |
| INV-12 | Projection pushdown cannot remove live columns |
| INV-13 | Predicate pushdown cannot change the observable result set |
| INV-14 | Cost estimates do not override cardinality contracts |
| INV-15 | Stats version changes invalidate dependent cache slots |

## Validation

```
cargo test -p andromeda-srpl --quiet
cargo test -p andromeda-catalog --quiet
cargo clippy -p andromeda-srpl -- -D warnings
cargo clippy -p andromeda-catalog -- -D warnings
```

All tests introduced in `v1-srpl-constant-folding-s03-design` Section 14
(T-CF-01 through T-INT-08) must pass before DEC-039 is closed.

## Invariants Preserved

- Procedure contracts remain typed, versioned, and hashed.
- Contract/binding mismatches are rejected before transaction creation.
- Plan decisions remain traceable by requirement.
- No ad hoc SQL, gRPC, or runtime JSON default is introduced.
- Protobuf remains a boundary format only.
- The catalog and srpl crates remain unsafe-free.
- `PlanClass` taxonomy is not extended.
- `MAX_SRPL_BODY_OPERATIONS = 16` is not increased.
