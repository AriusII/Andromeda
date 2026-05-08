# ContractCompatibility v0 Specification

## Purpose

Define the roadmap contract for `ContractCompatibility v0`, the classification
and diagnostic model used to decide whether a Procedure contract transition is
additive, breaking, deprecated, unchanged, or rejected.

Application behavior in Andromeda is exposed through typed, cataloged
Procedures. Compatibility decisions must therefore be made from typed contract
evidence, canonical hashes, versioned policy evidence, and catalog lifecycle
state. They must not be inferred from application command text, names alone, or
runtime object layout.

## Scope

This specification applies to Procedure contract compatibility checks used by
catalog publication, DefinitionBatch dry-run, SRPL binding, RPC admission,
security admission, plan-cache invalidation, audit evidence, and recovery
explanation.

It covers:

- compatibility inputs and validation order;
- additive, breaking, deprecated, unchanged, and rejected lifecycle classes;
- current `ExactHash` and `AdditiveOnly` behavior;
- compatibility diagnostic requirements;
- catalog-diff integration;
- implementation status and validation gates.

## Non-goals

This specification does not:

- introduce application-facing ad hoc SQL, dynamic predicates, dynamic table
  names, or shape-shifting returns;
- bypass typed Procedure contracts, `ContractHash`, or
  `ProcedureContractBinding`;
- define SRPL grammar, parser internals, binder internals, or IR lowering;
- define RPC frame bytes, Protobuf descriptor bytes, page formats, or durable
  WAL bytes;
- serialize Rust native structs directly to disk or network;
- treat RAM, audit records, temp files, GPU output, or benchmark output as
  compatibility truth;
- expose Administration or HA/DR capabilities through the Application surface;
- claim that the final dedicated compatibility crate has already been
  extracted.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `documentations/specs/ProcedureContract_v0.md`
- `documentations/specs/ContractHash_Canonicalization_v0.md`
- `documentations/specs/CatalogObjectModel_v0.md`
- `documentations/specs/DefinitionBatch_v0.md`
- `documentations/specs/CatalogDiff_v0.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `documentations/specs/PlanCacheKey_v0.md`
- `crates/andromeda-contract/src/contracts/types.rs`
- `crates/andromeda-contract/src/contracts/materialization.rs`
- `crates/andromeda-contract/src/contracts/hash.rs`
- `crates/andromeda-contract/src/contracts/validation.rs`

## Procedure

### Ownership

`andromeda-contract` owns the current Procedure contract descriptors,
canonical `ContractHash`, `PolicyVersion`, `ProcedureContractBinding`, and
local compatibility diagnostics.

`andromeda-catalog` owns recording the compatibility decision as catalog
evidence and enforcing publication, lifecycle, dependency, WAL, recovery, and
plan-cache consequences.

Future implementation may extract a dedicated compatibility plane or crate, but
that extraction must preserve the current Procedure-only contract boundary and
must not create an application-facing compatibility command language.

### Compatibility inputs

A compatibility decision must receive typed evidence, not raw command text.

| Input | Required rule |
| --- | --- |
| Previous contract | Must be a valid, canonical `ProcedureContract` when the object existed previously. |
| Next contract | Must be a valid, canonical `ProcedureContract` when the object remains active. |
| Lifecycle action | Must distinguish create, alter, deprecate, rename, move, restore, and rejected operation classes. |
| Previous binding | Must include `ProcedureId`, `CatalogVersion`, `ContractHash`, `StatsVersion`, and `PolicyVersion` when previous invocation evidence is relevant. |
| Next binding | Must include the same full binding evidence for the next active version. |
| Compatibility policy | Must be explicit. Current values are `ExactHash` and `AdditiveOnly`. |
| Dependency evidence | Must identify required object ids, version ranges, and edge kinds. |
| Publication evidence | Must tie accepted compatibility to a DefinitionBatch, operation index, target catalog version, and durable WAL evidence before visibility. |

The previous and next Procedure contracts must both pass canonical validation
before the compatibility class is accepted. A stale or non-canonical hash is a
compatibility rejection.

### Lifecycle classes

Compatibility lifecycle classes describe how a Procedure contract transition
affects existing and future callers.

| Class | Meaning | Required result |
| --- | --- | --- |
| `Unchanged` | The next contract advances catalog publication without changing compatibility-relevant evidence. | Existing bindings remain valid when full binding evidence still matches policy. |
| `Additive` | The next contract adds compatible surface without invalidating existing callers. | Existing compatible callers remain valid; new callers bind the new catalog evidence. |
| `Breaking` | The next contract changes identity, invocation shape, result contract, permission, policy, dependency, or binding evidence in a way existing callers cannot safely assume. | New binding or explicit migration is required. |
| `Deprecated` | Active visibility ended for the Procedure version while historical evidence remains. | New invocations must not bind to the deprecated version. |
| `Rejected` | Validation, compatibility, dependency, or durability checks failed. | No catalog visibility change is accepted. |

The lifecycle class must be recorded with the catalog object version evidence.
It must not be reconstructed later from name matching or from the existence of
a newer object version alone.

### Identity gates

Compatibility classification must fail closed before checking call-shape
details when identity evidence is invalid.

Required identity gates:

1. Both sides must validate canonical hash evidence when both sides exist.
2. Procedure qualified name must match unless an explicit rename or move rule
   supplies identity-preserving evidence.
3. `ProcedureId` must match for an alter-style compatibility decision.
4. `CatalogObjectId` must match for an object version replacement.
5. Object kind must be `Procedure` on both sides.
6. The next `CatalogVersion` must advance beyond the previous
   `CatalogVersion`.

Failure of any identity gate is breaking or rejected. It must not be classified
as additive.

### ExactHash policy

`CompatibilityPolicy::ExactHash` is the strict compatibility policy.

The transition is compatible only when:

- both contracts are valid and canonical;
- identity gates pass;
- the next `CatalogVersion` advances;
- the previous and next `ContractHash` values are equal.

Any contract hash mismatch under `ExactHash` is breaking. This is true even if
the change appears harmless to a caller. A future broader rule must use a
different explicit compatibility class and must preserve diagnostic evidence.

### AdditiveOnly policy

`CompatibilityPolicy::AdditiveOnly` permits a narrow set of compatible
extensions.

The current implemented additive rule accepts appended result stream columns
when all of these conditions hold:

1. Identity gates pass.
2. Both contracts are valid and canonical.
3. Inputs are unchanged.
4. Structured inputs are unchanged.
5. Required permissions are unchanged.
6. `StatsVersion` is unchanged.
7. Protocol layout is unchanged.
8. Transaction policy is unchanged.
9. Result metadata policy is unchanged.
10. Error policy is unchanged.
11. Multi-result policy is unchanged.
12. Existing result streams are not removed.
13. Existing result stream ids are unchanged.
14. Existing result stream cardinality is unchanged.
15. Existing result stream columns remain an unchanged prefix.
16. New result columns, when present, are appended after the unchanged prefix
    and use valid dense ordinals.

This policy is intentionally conservative. It does not currently accept input
extensions, permission changes, transaction policy changes, protocol changes,
stats changes, stream removal, stream cardinality changes, result stream id
changes, or reordering of existing output columns.

### Breaking changes

A Procedure contract transition is breaking when any compatibility-relevant
evidence changes outside the accepted additive rule.

Breaking examples:

| Change | Reason |
| --- | --- |
| Procedure id, object id, object kind, or qualified name drift | Existing bindings no longer identify the same Procedure object version. |
| Non-advancing `CatalogVersion` | Compatibility requires an explicit newer version. |
| Stale stored `ContractHash` | Contract evidence is non-canonical. |
| Input or structured input change | Invocation shape changed. |
| Required permission change | Security admission changed. |
| `StatsVersion` change | Planning and evidence identity changed. |
| `PolicyVersion` change | Policy-relevant behavior changed. |
| Protocol layout change | RPC payload contract changed. |
| Transaction policy change | Transaction semantics changed. |
| Result metadata policy change | ResultStream framing expectations changed. |
| Error policy change | Failure and rollback contract changed. |
| Multi-result policy change | Result shape contract changed. |
| Result stream removal | Existing output binding disappeared. |
| Result stream id change | Stable output identity changed. |
| Result stream cardinality change | Caller row-count expectations changed. |
| Existing result column reorder or type change | Existing output shape changed. |

Breaking changes may still be publishable through an explicit migration path,
but they must not be accepted as additive compatibility.

### Deprecated lifecycle

Deprecation ends active visibility for a Procedure object version and preserves
history.

Required behavior:

| Area | Rule |
| --- | --- |
| New invocation admission | Must reject binding to the deprecated active name or deprecated object version. |
| Historical lookup | Must preserve old object version, binding, contract hash, policy, and dependency evidence. |
| Dependency closure | Must reject deprecation when active dependents still require the object unless a reviewed policy explicitly handles them. |
| Plan cache | Must fence or invalidate keys that bind to the deprecated version for new invocations. |
| WAL and recovery | Must preserve durable evidence for deprecation and replay the same lifecycle transition. |
| Audit | Must record principal, batch, operation index, and diagnostic correlation when required by the surface. |

Deprecation is not an additive compatibility class for new callers. It is a
lifecycle state that may coexist with historical compatibility evidence.

### Diagnostic model

Compatibility diagnostics must be stable, typed, sanitized, and suitable for
catalog audit evidence.

| Diagnostic code | Meaning | Required evidence |
| --- | --- | --- |
| `CCOMPAT-CANONICAL-HASH-MISMATCH` | Stored Procedure hash does not match canonical contract hash. | Expected hash, actual hash, object id. |
| `CCOMPAT-IDENTITY-DRIFT` | Procedure name, id, object id, or object kind drifted. | Previous identity, next identity, field name. |
| `CCOMPAT-VERSION-NON-ADVANCING` | Next catalog version does not advance. | Previous version and next version. |
| `CCOMPAT-EXACT-HASH-CHANGED` | `ExactHash` policy saw a changed `ContractHash`. | Old hash, new hash. |
| `CCOMPAT-INPUT-SHAPE-CHANGED` | Input columns or structured inputs changed. | Field class and bounded shape summary. |
| `CCOMPAT-OUTPUT-SHAPE-BROKEN` | Existing ResultStream, column prefix, id, or cardinality changed. | Stream name or id and reason. |
| `CCOMPAT-PERMISSION-CHANGED` | Required permissions changed. | Old policy digest and new policy digest. |
| `CCOMPAT-POLICY-CHANGED` | Transaction, metadata, error, multi-result, stats, or policy evidence changed. | Changed policy field and binding evidence. |
| `CCOMPAT-DEPRECATED` | The Procedure version was deprecated. | Object id, deprecation version, dependency closure result. |
| `CCOMPAT-UNSUPPORTED-RULE` | The engine encountered a compatibility rule it cannot classify. | Rule id, object id, operation index. |

Current implementation returns `ContractCompatibilityDiagnostic` with a Boolean
and string messages. Future typed diagnostics must preserve the current failure
meaning while adding stable codes and bounded structured fields.

Diagnostic messages must not include raw secrets, raw credentials, private
keys, unbounded SRPL source, or host-specific absolute paths unless the path is
part of an explicit operator-selected input.

### Catalog integration

Compatibility is accepted only as part of catalog publication evidence.

Required catalog evidence:

1. `DefinitionBatchId` and operation index that requested the change.
2. Previous and next `CatalogVersion`.
3. Previous and next `CatalogObjectId` and object kind.
4. Previous and next `ContractHash` for Procedure transitions.
5. Previous and next `StatsVersion` and `PolicyVersion` when present.
6. Compatibility class and diagnostics.
7. Dependency closure decision.
8. Plan-cache impact.
9. WAL durable evidence before active publication.

Dry-run compatibility output is validation evidence. It does not become visible
catalog truth until apply emits deterministic catalog WAL records, durable WAL
coverage is confirmed, and catalog publication advances.

### Admission and plan-cache rules

Compatibility decisions must be enforced before transaction creation and before
plan reuse.

| Compatibility class | Invocation admission | Plan-cache behavior |
| --- | --- | --- |
| `Unchanged` | Accept when full binding evidence matches. | Reuse only when all key identities match. |
| `Additive` | Accept existing compatible bindings and new target bindings according to policy. | Existing keys may remain only when bound identities still match; target version creates new evidence. |
| `Breaking` | Reject stale bindings and require new binding or explicit migration. | Invalidate or fence affected keys. |
| `Deprecated` | Reject new invocations for the deprecated object version. | Fence keys for new invocation; keep historical evidence for recovery and audit. |
| `Rejected` | Reject the operation before publication. | Do not mutate cache truth. |

## Validation

Documentation acceptance checks:

- The spec preserves Procedure-only, typed contract compatibility.
- The spec defines additive, breaking, deprecated, unchanged, and rejected
  lifecycle classes.
- The spec documents the current `ExactHash` and `AdditiveOnly` behavior.
- The spec requires canonical hash validation before compatibility acceptance.
- The spec requires stable, sanitized diagnostics and catalog evidence.
- The spec does not introduce ad hoc SQL, runtime JSON defaults, native Rust
  struct serialization, or GPU work in critical paths.

Current focused implementation evidence:

```powershell
cargo test -p andromeda-catalog --test catalog_diff_contract
cargo test -p andromeda-contract --test procedure_contract_compat
```

Future implementation work should add or keep targeted validation for:

```powershell
cargo test -p andromeda-catalog --test catalog_diff_contract
cargo test -p andromeda-catalog --test batch_alter_drop_compat
cargo test -p andromeda-catalog --test plan_invalidation
cargo test -p andromeda-contract --test procedure_contract_compat
cargo test -p andromeda-contract --test contract_hash_golden
```

### Validation matrix

| Scenario | Expected result | Evidence |
| --- | --- | --- |
| Same Procedure identity, advancing version, same hash under `ExactHash`. | Compatible. | Contract compatibility tests. |
| Rehashed shape change under `ExactHash`. | Breaking diagnostic. | Contract compatibility tests. |
| Appended output column under `AdditiveOnly`. | Additive compatible. | Contract compatibility tests and catalog diff contract tests. |
| Input shape change under `AdditiveOnly`. | Breaking diagnostic. | Contract compatibility tests and catalog diff contract tests. |
| Existing result column reorder. | Breaking diagnostic. | Contract compatibility tests. |
| Procedure id or object id drift. | Breaking diagnostic. | Catalog digest and compatibility tests. |
| Procedure deprecation. | Deprecated lifecycle evidence and no new invocation binding. | DefinitionBatch deprecate dry-run tests. |
| Non-canonical stored hash. | Rejected compatibility. | Contract hash golden tests. |

## Current Implementation Status

Implemented:

- `andromeda-contract` defines `CompatibilityPolicy::ExactHash` and
  `CompatibilityPolicy::AdditiveOnly`.
- `ProcedureContractCandidate::materialize()` computes a canonical
  `ContractHash` and validates full binding evidence.
- `ProcedureContract::compatibility_with()` returns current compatibility
  diagnostics.
- `ProcedureContract::policy_version()` and `ProcedureContract::binding()`
  expose policy and full binding evidence.
- `andromeda-catalog` reexports the current contract compatibility facade for
  catalog integration.

Pending or partial:

- Compatibility diagnostics are not yet stable typed records with diagnostic
  codes.
- A dedicated compatibility plane or crate remains roadmap work.
- Full compatibility evidence persistence through catalog publication, WAL,
  recovery, subscription, and audit remains future integration work.
- Rename, move, restore, explicit alter, and explicit drop compatibility
  records remain beyond the current create and deprecate DefinitionBatch
  surface.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| A changed Procedure input is marked additive. | Compatibility logic ignored invocation shape. | Reject as breaking and emit an input-shape diagnostic. |
| `ExactHash` accepts a changed contract hash. | Policy was bypassed or hash validation was skipped. | Recompute canonical hash and reject under `ExactHash`. |
| Compatibility accepts a non-advancing version. | Version gate ran after shape comparison or was omitted. | Reject before classifying shape changes. |
| Deprecation erases old contract evidence. | Lifecycle transition was modeled as physical delete. | Preserve historical object version and binding evidence. |
| Plan cache reuses a stale Procedure binding. | Cache impact was not tied to compatibility class. | Bind plan-cache invalidation to full Procedure binding evidence. |
| Diagnostics expose source or secret material. | Error rendering used unbounded input text. | Use stable codes, bounded summaries, and sanitized evidence fields. |

## References

- `documentations/specs/ProcedureContract_v0.md`
- `documentations/specs/ContractHash_Canonicalization_v0.md`
- `documentations/specs/CatalogObjectModel_v0.md`
- `documentations/specs/DefinitionBatch_v0.md`
- `documentations/specs/CatalogDiff_v0.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `documentations/specs/PlanCacheKey_v0.md`
- `crates/andromeda-contract/src/contracts/types.rs`
- `crates/andromeda-contract/src/contracts/materialization.rs`
- `crates/andromeda-contract/src/contracts/hash.rs`
- `crates/andromeda-contract/src/contracts/validation.rs`
- `crates/andromeda-contract/tests/procedure_contract_compat.rs`
- `crates/andromeda-catalog/tests/catalog_diff_contract.rs`
