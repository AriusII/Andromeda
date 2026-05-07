# DEC-023 Drop Procedure Lifecycle Semantics

## Status

Design/Guard for V0.5. Implementation remains deferred until the DefinitionBatch mutation surface is
explicitly extended.

## Context

DefinitionBatch is the only catalog mutation path. A batch must parse, canonicalize, build a
dependency graph, dry-run, transactionally apply, and then publish a new `CatalogVersion`. DEC-022
defined semantics for `Alter Procedure`; this decision records the required semantics for a future
`Drop Procedure` operation without implementing it.

A Procedure lifecycle must support not only creation and modification, but also removal. However,
removal raises critical questions about:

1. **Identity transitions**: How do we mark a Procedure as removed while preserving historical
   evidence?
2. **Dependency management**: What happens to dependents when a Procedure is dropped?
3. **Active invocation boundary**: How do we prevent new invocations while allowing in-flight
   invocations to complete?
4. **Historical contract retention**: How long must we retain the dropped contract for audit and
   recovery?
5. **Compatibility guarantees**: How do we preserve contract binding evidence after drop?

This decision establishes constraints on Drop behavior before a `Drop` operation variant is added
to the mutation surface.

## Decision

`Drop Procedure` is a lifecycle operation that transitions a Procedure from **active** to
**inactive→removable** state. It is not `Deprecate`, because it removes all invocation
visibility, and it is not `Create`, because it targets an existing Procedure identity. Drop is
restricted, cascade-aware, and subject to dependency policy.

When implemented, a `Drop Procedure` DefinitionBatch operation MUST:

1. Target an existing active Procedure at the batch `base_version`.
2. Preserve the logical identity:
   - same `CatalogObjectId`;
   - same `QualifiedName`;
   - same `ProcedureId`;
   - `ObjectKind::Procedure`;
   - no new contract version (drop produces no new contract hash).
3. Validate that no other object in the batch **both creates or alters a dependency on the
   dropped Procedure AND that dependency cannot exist in the dropped state**. This means:
   - If a future Procedure-to-Procedure dependency model is added, new Procedures or altered
     Procedures must not depend on a Procedure being dropped in the same batch.
   - If in-batch creation/alteration causes a dependency to become invalid after the drop, the
     batch is rejected during dry-run.
4. Enforce cascade/restrict policy:
   - **Restrict mode** (default and only currently legal mode): reject the drop if any active
     dependency references this Procedure from another object.
   - **Cascade mode** (future, requires separate decision): cascade the drop to all dependents,
     dropping them in topological order.
5. Record the drop target identity, previous contract evidence, and inactive marking as part of
   the batch plan, but do **not** produce a new `CatalogVersion`-advancing contract for the
   Procedure.
6. Mark the Procedure as **inactive** in the catalog state, making it invisible to new invocation
   binding queries but preserving all historical contract evidence.
7. Emit recoverable and observable mutation/audit evidence when apply support is added.

## Identity Transition: Active → Inactive → Removable

The Procedure lifecycle comprises three states:

- **Active**: The Procedure is visible for binding and invocation. It has an active `ProcedureStoreEntry`.
- **Inactive**: The Procedure is no longer visible for new binding queries, but historical
  contract evidence remains queryable for audit and recovery. The `ProcedureStoreEntry` is
  removed from the active store. Historical versions and bindings remain in the catalog.
- **Removable**: (Future) The Procedure may be physically purged from persistent storage, subject
  to retention policy and WAL checkpoint boundaries. This requires a separate lifecycle decision.

A `Drop Procedure` operation produces a state transition from **Active** to **Inactive**.

The drop operation records:
- Drop timestamp and batch boundary.
- Target Procedure identity and all historical contract versions up to the drop point.
- The previous active `ContractHash`, `CatalogVersion`, `StatsVersion`, and `PolicyVersion`.
- Dependency cascade/restrict policy and evaluation result.
- Audit trail marker showing the drop event.

After drop publication, new invocation binding queries MUST NOT include the dropped Procedure in
their visible catalog set. In-flight invocations that bound before the drop remains valid; they
carry the pre-drop `CatalogVersion` and `ContractHash` and continue to execute under the old
contract.

## Cascade/Restrict Policy

Drop Procedure enforces a **restrict/cascade policy** similar to relational foreign-key semantics:

### Restrict Mode (Default, Required Implementation)

- If any other catalog object (currently: Table, StructuredObject, or future Procedure-to-Procedure
  binding) has an active reference or dependency to the dropped Procedure, drop is **rejected**.
- The batch dry-run must report:
  - the drop target identity;
  - the count and identity of blocking dependents;
  - the kind of each dependency (reads, writes, uses input, emits output, etc.).
- The batch fails before transaction creation; no mutation is applied.

### Cascade Mode (Future Decision Required Before Implementation)

- Drop may include an explicit `CascadePolicy::Cascade` flag (future decision only).
- If cascade is requested and legal, dependent objects are dropped in reverse topological order
  within the same batch.
- Each cascaded drop must record its trigger (dropped by cascade from parent).
- Cascaded drops must validate that no explicit batch operation tries to create or alter the
  cascaded dependent (conflict detection).
- Cascade must preserve audit evidence of the cascade chain: each dropped object records its
  parent in the drop cascade.

**Important**: Cascade mode is **out of scope for this decision**. It requires a separate decision
before implementation. All current Drop support must default to Restrict only.

## Historical Contract Retention

A dropped Procedure **retains all historical contract evidence**. The drop does not erase or
rewrite catalog history.

After drop:

- The Procedure is no longer visible in active binding queries (`ProcedureStoreEntry` removed).
- The Procedure's entire version history remains queryable for audit.
- All previous `ContractHash`, `CatalogVersion`, `StatsVersion`, and `PolicyVersion` tuples
  remain available.
- The drop event itself is recorded as an audit marker in the batch mutation plan.

This allows:
- **Audit tracing**: Review which invocations used which contracts before the procedure was dropped.
- **Recovery validation**: Verify that in-flight transactions can continue binding to their
  pre-drop contracts.
- **Compliance**: Prove that historical procedure definitions and invocations were valid at the
  time they occurred.

## Active Invocation Boundary

An invocation binds to a Procedure by `ProcedureId`, `ContractHash`, `CatalogVersion`,
`StatsVersion`, and `PolicyVersion` **before transaction creation**. Binding queries are
performed at bind time against the current active catalog.

Drop publication must preserve the invocation boundary:

1. **Before drop**: An invocation's bind-time query includes the Procedure in the active catalog.
   The invocation binds to its contract and may proceed.
2. **At drop**: The batch publishes the drop via a new `CatalogVersion` (for dependency/audit
   metadata), but the Procedure's contract is not replaced—it is marked inactive.
3. **After drop**: New bind-time queries do not include the dropped Procedure in active results.
   New invocations cannot bind to it.
4. **In-flight invocations**: Invocations that already bound (before the drop became visible) carry
   their bind-time `CatalogVersion` and `ContractHash`. They continue to execute using the
   pre-drop contract. The dropped state does not invalidate their binding.

**Key**: The drop does not retarget in-flight invocations. It only hides the Procedure from new
binding. Historical bindings remain valid because the contract evidence is preserved.

## Compatibility Guarantee Lifetime

After a Procedure is dropped, new code cannot depend on it; however, **historical invocation
evidence must remain compatible with the dropped contract for the lifetime of the record**.

This means:

- The dropped Procedure's `ContractHash`, `ProtocolLayoutRef`, `TransactionPolicy`,
  `ProcedureContractBinding`, and all result metadata remain immutable in the catalog.
- Audit and WAL recovery processes can still use the historical contract to validate dropped
  Procedure invocations.
- A future audit query can ask: "Was this invocation valid under the contract version in effect
  when it ran?" and obtain a definitive answer by referencing the preserved contract.

No compatibility policy (ExactHash, AdditiveOnly, etc.) is enforced on the drop itself because
drop is not a contract replacement. However, any existing compatibility guarantees from the
procedure's previous Alter operations remain valid for historical evidence.

## Active Invocation Visibility Gate

Before applying a Drop:

- Any Procedure-to-Procedure dependency (if added in a future decision) must not have new
  references from other Procedures being created or altered in the same batch.
- In-batch Created or Altered Procedures must not end up with an unsatisfiable dependency on the
  dropped Procedure.

This is validated during dry-run:

```
For each dropped Procedure in the batch:
  For each other object created or altered in the batch:
    If the object depends on the dropped Procedure:
      If the dependency is required (not optional):
        Reject the batch: "cannot create/alter object with unsatisfiable dependency"
```

This prevents logical inconsistencies where a batch both drops a Procedure and creates a
Procedure that depends on it (a contradiction).

## Plan-Cache Implications

DEC-016 defers any runtime plan cache. If a cache is later implemented, Drop imposes the
following constraints:

- A dropped Procedure must **never** produce a new cache entry after drop publication.
- Pre-drop cache entries (keyed by `ProcedureId`, `ContractHash`, `CatalogVersion`, etc.) remain
  valid for in-flight invocations that bound before the drop.
- The drop event must be observable to the cache layer so it can refuse to create new entries for
  the dropped Procedure.
- Invalidation of cache entries is automatic because the Procedure is no longer queryable in
  active bindings (no new invocations can create cache misses for it).

## Statistics and Feedback Retention

Statistics (`ProcedureFeedback`) for a dropped Procedure:

- Remain available in the feedback store for audit.
- Must be queryable by Procedure identity (because the identity is preserved even though the
  Procedure is inactive).
- New invocations cannot accumulate feedback for the dropped Procedure.
- Existing feedback records remain valid evidence of past performance.

If a procedure-feedback purge or archive operation is implemented, dropping a Procedure does not
automatically trigger purge; feedback retention is governed by a separate policy decision.

## Future Audit/WAL Requirements

When Drop apply support is implemented, catalog WAL/audit evidence must include at least:

- definition batch id;
- database id and namespace id;
- drop decision timestamp;
- target Procedure identity (`CatalogObjectId`, `QualifiedName`, `ProcedureId`);
- previous `ContractHash`, `CatalogVersion`, `StatsVersion`, and `PolicyVersion`;
- drop cascade/restrict policy and policy enforcement result;
- dependent object count and identity (if any were evaluated);
- whether the drop caused a batch rejection (restrict mode, blockers found);
- publication boundary and commit record evidence;
- state transition marker (Active → Inactive).

Recovery must apply Drop only from a fully committed begin/apply/commit mutation sequence.
Incomplete, sparse, duplicate, mismatched, or rejected Drop records must be skipped or reported
with catalog recovery anomalies, matching the existing recovery doctrine.

Recovery must replay drops in the correct order to reconstruct the inactive Procedure set:

- If a drop record appears in the WAL, the Procedure must be transitioned to Inactive during
  recovery.
- If the drop record is incomplete (missing commit marker), the drop is skipped and the Procedure
  remains active.

## Invariants Preserved

- DefinitionBatch remains the only catalog mutation path.
- Procedure contracts remain typed, versioned, and canonically hashed.
- Contract and dependency mismatches are rejected before transaction creation.
- Catalog history remains recoverable; dropped Procedure evidence is not erased.
- No ad hoc SQL, SQL-like runtime surface, gRPC, unsafe code, or runtime JSON default is
  introduced.
- Runtime plan cache remains deferred per DEC-016.
- Drop operations are atomic (all-or-nothing) within a batch.
- Restrict mode is the only legal default; cascade requires a future decision.

## Interaction with E2 (Alter Procedure) and E4/E6/E7

**E2 (Alter Procedure)**: Alter and Drop are complementary lifecycle operations:
- Alter modifies a Procedure's contract while keeping it active and visible.
- Drop makes a Procedure inactive (invisible for new binding).
- The same batch may not both Alter and Drop the same Procedure (conflict).
- The same batch may Alter one Procedure and Drop another (independent operations on different
  targets).

**E4 (Restrict/Cascade Lifecycle Decision)**: Drop's cascade/restrict policy depends on E4.
- This decision establishes that Drop must default to Restrict only.
- E4 must make a formal decision on whether Cascade is legal, under what conditions, and what
  semantics it entails.
- Drop cannot implement Cascade until E4 is accepted and constrains its behavior.

**E6/E7 (Future lifecycle phases)**: Drop is the removal operation. E6/E7 may define Retention
policies (when and how to physically purge dropped Procedures from storage) and Archive policies
(when to move dropped Procedures to cold storage). These are out of scope for this decision.

## Guard Tests and Validation

Until implementation, the catalog operation surface remains guarded as `Create`/`Deprecate` only.
The following tests must verify the guard and decision coverage:

1. **Decision text coverage**: The decision record must cover all required topics (see below).
2. **Operation surface guard**: `DefinitionOperation` must remain limited to `Create` and
   `Deprecate` variants; `Drop` is not yet a variant.
3. **Cascade/restrict framework**: A decision coverage test may check that the decision text
   mentions "Restrict mode", "Cascade mode", "out of scope for this decision", and "future E4
   decision" to establish the guard on cascade semantics.
4. **Dependency validation framework**: Tests may verify that in-batch dependency cycle detection
   and unsatisfiable dependency rejection work for `Create` and `Deprecate`; the Drop-specific
   validations are guarded until Drop is added to the operation surface.

## Validation

Until implementation, the catalog operation surface remains guarded as `Create`/`Deprecate` only.
Validation for this decision is:

- `cargo test -p andromeda-catalog --quiet`
- `cargo test -p andromeda-catalog --test decision_coverage_invariants --quiet` (extended to
  cover DEC-023 topics)

## Required Decision Outcomes Before Drop Implementation

Before any `Drop Procedure` surface is added to DefinitionBatch, the following must be resolved:

1. **E4 (Restrict/Cascade Policy)**: Must formally accept or defer cascade semantics. Drop
   implementation must default to Restrict only; any Cascade support requires E4 acceptance.
2. **E6/E7 (Retention and Removal Policy)**: Must establish whether dropped Procedures move to
   the Removable state and under what conditions. This decision only covers Active → Inactive
   transition.
3. **Procedure-to-Procedure Dependencies**: If Procedure-to-Procedure bindings are added, their
   drop semantics must be defined (inherited from Drop, or separate decision?).

Until these are resolved, Drop remains a design-only decision.

## Decision Rationale

This decision separates the concerns of Drop from Cascade/Restrict policy (E4) and future
Retention/Removal policy (E6/E7). By doing so:

- Drop can be designed and proven safe without assuming cascade semantics (which remain a
  governance question).
- Cascade becomes a separate, controlled decision that applies to other lifecycle operations
  (Deprecate, or future operations).
- Historical contract evidence is preserved, allowing audit and recovery to remain correct
  regardless of future policy changes.
- The Active → Inactive transition is strictly defined, preventing new invocations while
  protecting in-flight invocations.
- The restrict/cascade design space is made explicit: developers know exactly which policy is in
  effect and what the implications are.
