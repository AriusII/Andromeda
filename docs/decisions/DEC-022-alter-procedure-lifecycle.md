# DEC-022 Alter Procedure Lifecycle Semantics

## Status

Accepted for the V0.5 design guard. Implementation remains deferred until the
DefinitionBatch mutation surface is explicitly extended.

## Context

DefinitionBatch is the only catalog mutation path. A batch must parse,
canonicalize, build a dependency graph, dry-run, transactionally apply, and then
publish a new `CatalogVersion`. The current operation surface is limited to
`Create` and `Deprecate`; this decision records the required semantics for a
future `Alter Procedure` operation without implementing it.

Every Procedure contract is typed, versioned, and hashed. A contract mismatch is
rejected before transaction creation. No procedure lifecycle operation may add
ad hoc SQL, a SQL-like native runtime surface, gRPC, unsafe runtime behavior, or
runtime JSON defaults.

## Decision

`Alter Procedure` is a versioned contract replacement for an existing Procedure
identity. It is not `Create`, because it does not allocate a new logical
procedure. It is not `Deprecate`, because it does not remove visibility or mark a
definition as obsolete.

When implemented, an `Alter Procedure` DefinitionBatch operation MUST:

1. Target an existing active Procedure at the batch `base_version`.
2. Preserve the logical identity:
   - same `CatalogObjectId`;
   - same `QualifiedName`;
   - same `ProcedureId`;
   - `ObjectKind::Procedure`.
3. Materialize a replacement `ProcedureContract` whose
   `object.catalog_version` equals the batch `next_version`.
4. Validate the replacement contract, including canonical `ContractHash`, before
   transaction creation.
5. Run compatibility diagnostics against the previous persisted contract and
   reject incompatible changes during dry-run.
6. Preserve the previous contract as catalog history; alter never rewrites or
   deletes past contract evidence.
7. Emit recoverable and observable mutation/audit evidence when apply support is
   added.

## Difference from Create

`Create` introduces a new catalog object at the planned next version. Its object
id and name must not already be changed by the batch, and the created definition
is validated as a new object.

`Alter Procedure` changes an existing Procedure identity. It must prove that the
target exists at `base_version`, that the replacement keeps the same logical
identity, and that the replacement contract is compatible with the previous
contract. The altered Procedure becomes visible only at `next_version`.

## Difference from Deprecate

`Deprecate` is a lifecycle transition for an existing object. It produces no new
contract hash and does not define a replacement callable interface.

`Alter Procedure` keeps the Procedure active and replaces the active contract for
new bindings after publication. Existing historical bindings remain valid only
for the contract/catalog version they captured.

## Compatibility Policy

The compatibility policy is evaluated from the replacement contract against the
previous contract using the catalog contract diagnostics:

- `ExactHash` permits only an unchanged canonical `ContractHash`. The operation
  may publish a new catalog version for the same contract evidence, but any
  contract-shape, protocol, policy, permission, stats, input, or result change
  is rejected.
- `AdditiveOnly` permits only the existing additive rules: inputs and structured
  inputs remain unchanged; permissions, stats version, protocol layout,
  transaction policy, result metadata policy, error policy, and multi-result
  policy remain unchanged; existing result streams remain present with unchanged
  ids, cardinality requirements, and existing columns as an unchanged prefix.
  Additional result streams or additive result columns are allowed only when the
  diagnostic accepts them.

Alter must also enforce identity equality (`CatalogObjectId`, `QualifiedName`,
`ProcedureId`) because the compatibility diagnostic currently checks the
Procedure name and contract shape, not the full catalog identity tuple.

## Version and ContractHash Behavior

Each accepted Alter advances the enclosing catalog from `base_version` to
`next_version`. The replacement `ProcedureContract.object.catalog_version` must
equal `next_version`.

The replacement `ContractHash` must equal the canonical hash of the replacement
contract shape. If the replacement shape or policy changes in a compatible way,
the hash changes and the new hash becomes the binding hash for new invocations.
If the replacement is `ExactHash`-compatible, the hash remains unchanged even
though the catalog version advances.

`StatsVersion` and `PolicyVersion` remain part of the Procedure binding and
future plan-cache identity. A change rejected by compatibility diagnostics must
not create a transaction.

## Dependency Traversal

Dry-run must compare the previous Procedure dependency set with the replacement
dependency set. For the current catalog model this means Procedure outgoing
dependencies to structured inputs and binding-derived table/structured-object
edges.

The replacement dependency graph must:

- validate all new dependency names and kinds;
- include same-batch creations or alterations when they are legitimate
  dependencies;
- reject missing dependencies before transaction creation;
- reject in-batch cycles;
- record dependency additions and removals as dry-run diagnostics and future
  audit evidence.

Alter does not cascade changes into dependent objects. If future Procedure-to-
Procedure dependencies are introduced, dependents must bind by explicit
`ContractHash`/`CatalogVersion` evidence and incompatible rebinding must be a
separate decision.

## Restrict/Cascade Interaction

`Alter Procedure` has no cascade mode. It is a restricted operation:

- compatible replacement: allowed;
- incompatible replacement: rejected;
- missing or invalid dependencies: rejected;
- identity drift: rejected.

Drop semantics and cascade semantics for removing objects remain out of scope for
this decision and must not be inferred from Alter. A future `Drop Procedure` or
general cascade policy requires its own decision before implementation.

## Active Invocation Boundary

An invocation binds to a Procedure by `ProcedureId`, `ContractHash`,
`CatalogVersion`, `StatsVersion`, and `PolicyVersion` before transaction
creation. Alter publication must not retarget an invocation that has already
bound to an older contract.

After the altered catalog version is published, new invocations must bind against
the newly visible Procedure contract. Old invocation evidence remains auditable
against the historical contract version.

## Plan-Cache Invalidation Implications

DEC-016 defers any runtime plan cache. If a cache is later implemented, Alter
does not require ad hoc invalidation logic to preserve correctness because the
cache key must include `ProcedureId`, `ContractHash`, `CatalogVersion`,
`StatsVersion`, `PolicyVersion`, plan class, and shape fingerprint.

Changing any key input separates cache slots. Even an `ExactHash` Alter advances
`CatalogVersion`, so old and new catalog-visible bindings cannot share a cache
entry unless a future decision explicitly proves safe reuse and traces it.

Future cache invalidation or reuse traces must record the Alter-driven key
evidence and must remain bounded and observable.

## Future Audit/WAL Requirements

When Alter apply support is implemented, catalog WAL/audit evidence must include
at least:

- definition batch id;
- database id and namespace id;
- previous and next catalog versions;
- operation index;
- target Procedure identity (`CatalogObjectId`, `QualifiedName`, `ProcedureId`);
- previous `ContractHash`, `StatsVersion`, and `PolicyVersion`;
- replacement `ContractHash`, `StatsVersion`, and `PolicyVersion`;
- compatibility policy and diagnostic result;
- dependency additions/removals;
- publication boundary and commit record evidence.

Recovery must apply Alter only from a fully committed begin/apply/commit mutation
sequence. Incomplete, sparse, duplicate, mismatched, or rejected Alter records
must be skipped or reported with catalog recovery anomalies, matching the
existing recovery doctrine.

## Invariants Preserved

- DefinitionBatch remains the only catalog mutation path.
- Procedure contracts remain typed, versioned, and canonically hashed.
- Contract and dependency mismatches are rejected before transaction creation.
- Catalog history remains recoverable; previous contract evidence is not erased.
- No ad hoc SQL, SQL-like runtime surface, gRPC, unsafe code, or runtime JSON
  default is introduced.
- Runtime plan cache remains deferred per DEC-016.

## Validation

Until implementation, the catalog operation surface remains guarded as
`Create`/`Deprecate` only. Validation for this decision is:

- `cargo test -p andromeda-catalog --quiet`
- `cargo test -p andromeda-catalog --test decision_coverage --quiet`
