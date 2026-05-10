# Specification: CatalogObjectModel v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the purpose and scope of `CatalogObjectModel v0`.
- State the required structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Define cataloged objects and their lifecycle.

## Scope

This specification applies to V0 documentation and implementation planning. It defines the minimum stable contract needed for code, tests, and review.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only surface.
- It does not authorize hidden dynamic behavior.

## Data structures

| Structure | Required role |
|---|---|
| `DatabaseDescriptor` | Must be represented as an explicit typed structure or canonical descriptor. |
| `NamespaceDescriptor` | Must be represented as an explicit typed structure or canonical descriptor. |
| `TableDescriptor` | Must be represented as an explicit typed structure or canonical descriptor. |
| `MapDescriptor` | Must be represented as an explicit typed structure or canonical descriptor. |
| `EnumDescriptor` | Must be represented as an explicit typed structure or canonical descriptor. |
| `StructuredObjectDescriptor` | Must be represented as an explicit typed structure or canonical descriptor. |
| `ProcedureDescriptor` | Must be represented as an explicit typed structure or canonical descriptor. |
| `CatalogVersion` | Monotonic published version identifier. |
| `CatalogObjectId` | Stable object identity independent from display name. |
| `ContractHashBinding` | Procedure descriptor binding to canonical contract hash. |
| `PublicationEvidence` | WAL-backed evidence for a visible catalog publication. |

### Descriptor identity model

Every cataloged object descriptor must carry explicit identity and version evidence before it can
be published.

| Field | Rule |
|---|---|
| `CatalogObjectId` | Non-zero stable identity. It does not change on compatible descriptor evolution. |
| `QualifiedName` | Durable display/resolution name scoped by database and namespace. It is not object identity. |
| `ObjectKind` | One of Database, Namespace, Table, Map, Enum, StructuredObject, or Procedure. The kind must match the descriptor variant. |
| `CatalogVersion` | Non-zero published version at which the descriptor becomes visible. |
| `ObjectVersion` | Descriptor-local version when maintained separately from `CatalogVersion`; must be non-zero and monotonic for the same `CatalogObjectId`. |
| `ShapeHash` | Canonical digest of the versioned descriptor shape used for diffing, compatibility, and evidence. |
| `LifecycleState` | Active, Deprecated, or Reserved. V0 does not use hidden tombstones. |
| `DependencyEdges` | Explicit object dependency edges with dependent object, dependency object, edge kind, and object kinds. |

Descriptor column or field lists must use stable names, explicit type descriptors, explicit absence
policy, unique names, and dense zero-based ordinals. The catalog must reject any descriptor that
requires positional-only interpretation.

### ProcedureDescriptor binding

`ProcedureDescriptor` must bind:

| Binding field | Rule |
|---|---|
| `ProcedureId` | Non-zero identity used by invocation and DefinitionBatch source evidence. |
| `CatalogObjectId` | Non-zero catalog object identity for the procedure descriptor. |
| `CatalogVersion` | Published catalog version for this descriptor. |
| `ContractHash` | Non-zero canonical ProcedureContract hash. |
| `StatsVersion` | Non-zero statistics evidence referenced by invocation binding. |
| `PolicyVersion` | Non-zero policy digest referenced by invocation binding. |
| `CompatibilityPolicy` | Policy used to classify transition from prior descriptor version. |
| `ProtocolLayout` | Protocol descriptor and frame envelope hashes required by RPC/result streaming. |

The descriptor is invalid if it has a `ContractHash` without matching canonical contract evidence,
or if it has contract evidence without durable catalog publication evidence.

## Invariants

- CatalogVersion is monotonic.
- Published descriptors are immutable.
- System Database is C5.
- Object dependencies are explicit.
- Procedure descriptors bind `ProcedureId`, `ContractHash`, `CatalogVersion`, and compatibility policy.
- A visible CatalogVersion requires durable publication evidence.
- Display names are not object identity.
- `CatalogObjectId`, object version, and published `CatalogVersion` are non-zero.
- Descriptor kind must match the owning descriptor variant.
- A compatible descriptor evolution preserves `CatalogObjectId` and qualified name unless a future rename/move operation defines explicit evidence; V0 rename and move are rejected.
- A DefinitionBatch may not change the same `CatalogObjectId` or qualified name more than once.
- New or changed descriptors in one DefinitionBatch must be operation-ordered by dependencies; forward intra-batch references are rejected.
- Dependency cycles are rejected unless a future version defines an explicit cycle policy and proves bounded initialization.
- Procedure dependencies include structured inputs, reads, writes, and emitted structured objects when those bindings exist.
- Deprecated descriptors retain historical evidence and may be used to validate old audit/recovery records; they must not be selected for new invocation binding.
- No visible catalog state may be reconstructed from RAM, traces, benchmarks, GPU output, or analytics projections.

### Dependency edge kinds

Catalog dependency edges must be explicit and typed.

| Edge kind | Dependent | Dependency | Rule |
|---|---|---|---|
| `NamespaceContainsObject` | Namespace | Catalog object | Object namespace membership. |
| `ProcedureStructuredInput` | Procedure | StructuredObject | Procedure accepts structured object input. |
| `ProcedureReadsTable` | Procedure | Table | Procedure reads table data. |
| `ProcedureWritesTable` | Procedure | Table | Procedure writes table data. |
| `ProcedureEmitsStructuredObject` | Procedure | StructuredObject | Procedure emits structured result object. |
| `MapReadsSource` | Map | Table or Map | Map source dependency. |
| `ObjectUsesEnum` | Table, StructuredObject, Procedure, or Map | Enum | Type dependency. |

The catalog must reject a dependency edge when the declared dependency kind does not match the
published or in-batch descriptor kind. If a dependency target is created in the same batch, the
target operation must precede the dependent operation.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

### Publication evidence format

Catalog publication is visible only after the catalog owner has durable evidence for the complete
DefinitionBatch mutation.

| Evidence field | Required rule |
|---|---|
| `DefinitionBatchId` | Non-zero batch identity shared by dry-run, apply, WAL records, audit, and recovery evidence. |
| `DatabaseId` and `NamespaceId` | Non-zero scope identities for the publication. |
| `PreviousCatalogVersion` | Version used as the dry-run and apply base. |
| `NextCatalogVersion` | Exact version made visible; must be greater than previous version. |
| `SourceHash` | Non-zero hash of accepted batch source or canonical import payload. |
| `DependencyGraphHash` | Non-zero hash of the accepted dependency graph. |
| `OperationCount` | Number of applied operation records. |
| `PublicationWalRecord` | Commit record or equivalent WAL-backed durable marker covering the full batch. |
| `DurableLsn` | LSN or external durable marker at or beyond the publication commit record. |
| `CompatibilityDecision` | Decision for every changed Procedure descriptor. |

The catalog mutation record sequence is Begin, one Apply record per operation in dense operation
order, then Commit with the same boundary as Begin. A visible `CatalogVersion` must not be advanced
until the Commit evidence is durable.

## State transitions

State transitions must be explicit. Invalid transitions return typed errors and emit trace evidence when they affect execution, storage, security, or recovery.

## Error model

| Error family | Use |
|---|---|
| ContractError | Invalid shape, incompatible hash, missing contract field. |
| PermissionError | Principal lacks required permission or surface scope. |
| ResourceError | Budget, quota, backpressure, or timeout failure. |
| TransactionError | Isolation, rollback, commit, or serialization failure. |
| StorageError | WAL, page, segment, manifest, or corruption failure. |
| SystemError | Internal condition requiring poison, rollback, forensic, or restore path. |

### CatalogRejectionCode v0

Implementations must expose stable typed rejection codes or stable diagnostic ids for these V0
failures.

| Code | Required use |
|---|---|
| `CATALOG_OBJECT_ID_MISSING` | `CatalogObjectId` is zero or absent. |
| `CATALOG_OBJECT_VERSION_MISSING` | Descriptor object version is zero or absent. |
| `CATALOG_VERSION_MISSING` | Published `CatalogVersion` is zero or absent. |
| `CATALOG_VERSION_NON_ADVANCING` | Publication does not advance from previous version. |
| `DESCRIPTOR_KIND_MISMATCH` | Descriptor variant and object kind disagree. |
| `DESCRIPTOR_NAME_DUPLICATE` | Batch or catalog contains duplicate active qualified names in one scope. |
| `DESCRIPTOR_ID_DUPLICATE` | Batch changes the same `CatalogObjectId` more than once. |
| `DESCRIPTOR_COLUMNS_NOT_DENSE` | Columns or fields are not dense and zero-based. |
| `DESCRIPTOR_COLUMN_NAME_DUPLICATE` | Descriptor has duplicate column or field names. |
| `PROCEDURE_DESCRIPTOR_CONTRACT_HASH_MISSING` | Procedure descriptor lacks non-zero `ContractHash`. |
| `PROCEDURE_DESCRIPTOR_BINDING_INCOMPLETE` | Procedure descriptor lacks procedure, catalog, contract, stats, policy, or protocol binding evidence. |
| `DEPENDENCY_MISSING` | Required dependency target is absent. |
| `DEPENDENCY_KIND_MISMATCH` | Dependency edge kind disagrees with target object kind. |
| `DEPENDENCY_FORWARD_REFERENCE` | In-batch dependency target appears after dependent operation. |
| `DEPENDENCY_CYCLE_UNSUPPORTED` | Dependency graph contains a cycle without a versioned cycle policy. |
| `PUBLICATION_EVIDENCE_MISSING` | Visible version lacks durable publication evidence. |
| `PUBLICATION_DURABILITY_NOT_REACHED` | Publication commit evidence is not in the durable prefix. |
| `HALF_CATALOG_PUBLICATION` | Recovery or apply observes incomplete Begin/Apply/Commit evidence. |
| `DISPLAY_NAME_USED_AS_IDENTITY` | Durable identity comparison uses display name instead of `CatalogObjectId`. |

## Security model

Security-sensitive operations require admission through identity, principal, permission, policy, and audit checks before durable mutation or transaction creation.

## Observability

At minimum, implementations must emit trace evidence with:

```text
TraceId
InvocationId when applicable
CatalogVersion when applicable
PolicyVersion when applicable
Result
ErrorKind when applicable
```

Catalog publication, recovery replay, and rejection evidence must also include:

```text
DefinitionBatchId
DatabaseId
NamespaceId
PreviousCatalogVersion
NextCatalogVersion
SourceHash
DependencyGraphHash
OperationIndex when applicable
CatalogObjectId when applicable
ContractHash when applicable
StatsVersion when applicable
PolicyVersion when applicable
DurableLsn or durable marker when applicable
CatalogRejectionCode when rejected
```

## Recovery behavior

Catalog state is durable state. Recovery rebuilds the visible catalog from the last valid durable
catalog snapshot plus complete committed catalog mutation records in the durable WAL prefix.

Recovery must replay a catalog mutation only when:

- Begin and Commit boundaries match.
- Apply record count equals the boundary operation count.
- `DefinitionBatchId`, database, namespace, previous version, next version, source hash, and dependency graph hash match across the boundary.
- `NextCatalogVersion` advances the recovered visible version exactly as the mutation declares.
- The commit record or external durable marker is inside the durable prefix.

Recovery must skip incomplete, mismatched, or non-durable mutation evidence and emit recovery
evidence. It must not publish a half-catalog. If the last visible catalog cannot be proven from
snapshot plus durable WAL evidence, startup must choose a fail-closed recovery mode rather than
inventing catalog truth.

## Compatibility

Changes are classified as:

| Change | Default status |
|---|---|
| Add optional field with explicit default | Additive |
| Add required field | Breaking |
| Change type or cardinality | Breaking |
| Change security requirement | Security-impact |
| Change recovery behavior | Breaking unless explicitly versioned |

### Catalog descriptor compatibility

| Change | V0 decision |
|---|---|
| Same descriptor shape at advancing `CatalogVersion` | Compatible metadata/version publication. |
| Append optional table or structured-object field with explicit default | Additive if owner-specific compatibility accepts it. |
| Add required field | Breaking and rejected by default. |
| Change existing field type, absence policy, ordinal, or name | Breaking and rejected by default. |
| Change `CatalogObjectId` for an existing object | Breaking identity drift and rejected. |
| Change `ObjectKind` for an existing object | Rejected. |
| Rename or move object | Reserved and rejected in V0. |
| Deprecate active descriptor | Allowed only through DefinitionBatch with dependency closure and retained evidence. |
| Drop descriptor and erase history | Reserved and rejected in V0. |
| Procedure descriptor change | Must also satisfy `SPEC_PROCEDURE_CONTRACT_V0.md` compatibility. |

Catalog descriptor compatibility is necessary but not sufficient for publication. Publication also
requires DefinitionBatch dry-run acceptance, dependency graph acceptance, durable WAL evidence, and
security/admission approval where policy changes are involved.

## Tests

- definition ordering tests.
- dependency graph tests.
- catalog rollback tests.
- object lifecycle transition tests.
- publication evidence tests.
- descriptor identity rename tests.
- non-zero `CatalogObjectId`, object version, and `CatalogVersion` tests.
- descriptor kind mismatch tests.
- dense column/field ordinal and duplicate name tests.
- dependency kind mismatch tests.
- forward intra-batch dependency rejection tests.
- dependency cycle rejection tests.
- ProcedureDescriptor binding completeness tests for `ProcedureId`, `ContractHash`, `StatsVersion`, `PolicyVersion`, and protocol layout.
- catalog mutation Begin/Apply/Commit boundary tests.
- recovery replay complete committed batch tests.
- recovery skip incomplete or non-durable batch tests.
- stable CatalogRejectionCode tests.

## Rejection criteria

- Reject `half-published catalog`.
- Reject `object without version`.
- Reject `ProcedureDescriptor without ContractHash`.
- Reject `CatalogVersion without durable publication evidence`.
- Reject `object dependency cycle without policy`.
- Reject `display name used as durable identity`.
- Reject `zero CatalogObjectId`.
- Reject `descriptor kind mismatch`.
- Reject `duplicate CatalogObjectId or qualified name changed more than once in one DefinitionBatch`.
- Reject `columns or fields without dense zero-based ordinals`.
- Reject `dependency kind mismatch`.
- Reject `forward intra-batch dependency reference`.
- Reject `ProcedureDescriptor without ProcedureId StatsVersion PolicyVersion or ProtocolLayout`.
- Reject `CatalogVersion advanced before durable publication commit evidence`.
- Reject `rename move or hard drop in V0`.
- Reject `deprecated descriptor selected for new invocation binding`.
- Reject `catalog truth reconstructed from RAM trace benchmark GPU or analytics evidence`.

## Implementation handoff

P03/P04 implementation work must use this handoff:

| Owner crate or boundary | Required responsibility |
|---|---|
| `andromeda-catalog` | Owns descriptor identity, dependency graph acceptance, publication evidence, visible `CatalogVersion`, and recovery replay/skip rules. |
| `andromeda-procedure-contract` | Owns `ProcedureContract`, `ContractHash`, `PolicyVersion`, binding validation, and Procedure compatibility classification used by ProcedureDescriptor publication. |
| DefinitionBatch | Owns ordered catalog mutation input, dry-run acceptance, source hash, dependency graph hash, and lifecycle operation vocabulary. |
| WAL/storage | Owns durable prefix, LSN evidence, and persisted mutation record integrity. |
| Execution admission | Resolves only active Procedure descriptors and rejects stale `CatalogVersion` or `ContractHash` binding evidence before transaction creation. |
| Audit/DecisionTrace | Retains publication, compatibility, rejection, and recovery evidence with stable ids and rejection codes. |

## Acceptance summary

Owner: `andromeda-catalog` owns descriptor identity, dependency graph acceptance, durable
publication evidence, visible `CatalogVersion`, and recovery replay/skip rules; Procedure contract
compatibility remains owned by `andromeda-procedure-contract` and is consumed by catalog
publication.

Evidence: acceptance requires descriptor identity tests, kind and ordinal rejection tests,
dependency graph ordering tests, ProcedureDescriptor binding completeness tests, publication
Begin/Apply/Commit boundary tests, durable LSN or marker evidence, and recovery tests proving
complete committed batches replay while incomplete or non-durable batches are skipped.

Reject: acceptance is not met if any implementation publishes a half-catalog, advances
`CatalogVersion` before durable publication evidence, treats display names as durable identity,
accepts zero object ids or versions, accepts dependency kind mismatches or forward references,
selects deprecated descriptors for new invocation binding, or reconstructs catalog truth from RAM,
traces, benchmarks, GPU output, or analytics evidence.
