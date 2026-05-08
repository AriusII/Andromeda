# ProcedureContract v0 Specification

## Purpose

Define the accepted documentation contract for `ProcedureContract v0`, the
typed, cataloged Procedure contract boundary used by Andromeda catalog, SRPL,
execution, protocol, plan-cache, security-admission, and observability paths.

`ProcedureContract v0` is a documentation baseline for the current
`andromeda-contract` behavior. It is not a claim that every runtime consumer has
completed full binding propagation.

## Scope

This specification applies to Procedure contracts materialized through
`andromeda-contract` and consumed by catalog publication, SRPL lowering,
Procedure invocation, protocol manifests, security admission, audit evidence,
plan-cache identity, and benchmark or scenario evidence.

It covers:

- Procedure identity and catalog object evidence;
- typed input and structured input descriptors;
- result stream shape, cardinality, and metadata policy;
- transaction, compatibility, error, and multi-result policies;
- required permission evidence;
- `ContractHash`, `CatalogVersion`, `StatsVersion`, and `PolicyVersion`
  binding;
- compatibility checks currently implemented in `andromeda-contract`;
- implemented versus pending gaps.

## Non-goals

This specification does not:

- introduce application-facing ad hoc SQL;
- allow dynamic table names, dynamic predicates, shape-shifting returns, or
  implicit null semantics;
- bypass typed Procedure contracts;
- define SRPL grammar, parser diagnostics, or full typed IR semantics;
- define the binary RPC frame format or StructuredObject wire payload format;
- define durable catalog WAL record bytes;
- make benchmark output, RAM state, GPU output, or audit evidence database
  truth;
- expose Administration or HA/DR capabilities through the Application surface;
- claim that every execution, audit, protocol, and plan-cache path already uses
  full `ProcedureContractBinding`.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `crates/andromeda-contract/src/contracts/types.rs`
- `crates/andromeda-contract/src/contracts/materialization.rs`
- `crates/andromeda-contract/src/contracts/hash.rs`
- `crates/andromeda-contract/src/contracts/validation.rs`
- `crates/andromeda-contract/src/objects.rs`
- `crates/andromeda-contract/src/objects/validation.rs`
- `crates/andromeda-types/src/ids.rs`
- `crates/andromeda-types/src/types.rs`
- `crates/andromeda-contract/tests/contract_hash_golden.rs`
- `crates/andromeda-srpl/tests/compiler_pipeline_e2e/contract_hash_canonical_forms.rs`
- `documentations/specs/ContractHash_Canonicalization_v0.md`
- `documentations/specs/FrameHeader_RPC_v0.md`
- `documentations/specs/SecurityAdmission_v0.md`

## Procedure

### Contract construction

Producers must create a Procedure contract through a typed candidate or an
equivalent path that computes the canonical hash before catalog publication.

The current implementation uses `ProcedureContractCandidate::materialize()` to:

1. Accept typed catalog, input, result, permission, protocol, and policy fields.
2. Compute `contract_hash` from the canonical Procedure contract hash inputs.
3. Construct `ProcedureContract`.
4. Validate the contract and full binding evidence.
5. Reject the candidate before publication if validation fails.

Callers that construct `ProcedureContract` directly must call
`validate_canonical_hash()` or `validated_binding()` before accepting the
contract as catalog evidence.

### Required fields

| Field | Current Rust type | Required rule |
| --- | --- | --- |
| `object` | `CatalogObjectRef` | Must be a non-zero catalog object reference of kind `Procedure` with non-zero `CatalogVersion`. |
| `procedure_id` | `ProcedureId` | Must be non-zero for active contracts and bindings. |
| `contract_hash` | `ContractHash` | Must be a non-zero 32-byte hash that equals `canonical_hash()`. |
| `stats_version` | `StatsVersion` | Must be non-zero. |
| `protocol_layout` | `ProtocolLayoutRef` | Must include non-zero descriptor-set and frame-envelope hashes. |
| `inputs` | `Vec<ColumnDescriptor>` | May be empty, but any present columns must validate and use dense zero-based ordinals. |
| `structured_inputs` | `Vec<QualifiedName>` | May be empty; names must be unique. |
| `result_streams` | `Vec<ResultStreamContract>` | May be empty; each stream must validate and stream ids and names must be unique. |
| `required_permissions` | `Vec<String>` | Must be non-empty; entries must be non-empty and unique. |
| `transaction_policy` | `TransactionPolicy` | Must declare access mode, isolation, and retryability. |
| `compatibility_policy` | `CompatibilityPolicy` | Must be `ExactHash` or `AdditiveOnly`. |
| `result_metadata_policy` | `ResultMetadataPolicy` | Must declare metadata-before-payload behavior. |
| `error_policy` | `ProcedureErrorPolicy` | Error codes must be non-empty when present and unique. |
| `multi_result_policy` | `MultiResultPolicy` | Must match the number of result streams. |

### Identity fields

`ProcedureContractRef` is the legacy three-part identity:

| Field | Rule |
| --- | --- |
| `procedure_id` | Non-zero. |
| `contract_hash` | Non-zero. |
| `catalog_version` | Non-zero. |

`ProcedureContractBinding` is the full binding evidence required for new
Procedure invocation and cross-engine evidence:

| Field | Rule |
| --- | --- |
| `procedure_id` | Must match the Procedure contract. |
| `catalog_version` | Must match `object.catalog_version`. |
| `contract_hash` | Must match the stored and canonical contract hash. |
| `stats_version` | Must match the Procedure contract and be non-zero. |
| `policy_version` | Must match the computed policy digest and be non-zero. |

New execution, plan-cache, audit, protocol-manifest, and benchmark evidence
must prefer `ProcedureContractBinding` over `ProcedureContractRef`. The legacy
reference remains a compatibility projection.

### Type and column rules

Input and result columns use `ColumnDescriptor`.

| Field | Rule |
| --- | --- |
| `name` | Must not be empty after trimming. |
| `data_type` | Must validate its scalar and absence policy. |
| `ordinal` | Must be dense and zero-based within the containing column list. |

Column names in a single column list must be unique. The current implementation
does not require the Procedure input column list to be non-empty, but result
stream column lists must be non-empty.

`TypeDescriptor.absence` must be explicit. `Required` means absence is not
permitted. `ExplicitOptional` means absence is part of the type contract. Do not
represent optional data through implicit null semantics.

### Result stream rules

Each `ResultStreamContract` must include:

| Field | Rule |
| --- | --- |
| `stream_id` | Must be non-zero and unique within the Procedure contract. |
| `name` | Must not be empty and must be unique within the Procedure contract. |
| `columns` | Must be non-empty, valid, unique by name, and dense by ordinal. |
| `cardinality` | Must be one of the stable cardinality tags. |
| `row_count_exact_required` | Must equal the deterministic legacy projection of `cardinality`. |

Current stable cardinality tags:

| Cardinality | Stable tag | Minimum rows | Intrinsic maximum rows | Legacy exact row count required |
| --- | ---: | ---: | --- | --- |
| `One` | 0 | 1 | 1 | Yes |
| `OptionalOne` | 1 | 0 | 1 | No |
| `Many` | 2 | 0 | Unbounded | No |
| `NonEmptyMany` | 3 | 1 | Unbounded | Yes |

If `multi_result_policy` is `SingleResultOnly`, the Procedure contract must not
declare more than one result stream. `MultipleResultStreamsAllowed` permits
multiple streams, subject to stream validation.

### Policy fields

| Policy | Current values | Required rule |
| --- | --- | --- |
| `AccessMode` | `ReadOnly`, `ReadWrite` | Must state whether the Procedure may mutate durable state. |
| `IsolationPolicy` | `Snapshot`, `Serializable` | Must state the transaction visibility contract. |
| `TransactionPolicy.retryable` | Boolean | Must be explicit. |
| `CompatibilityPolicy` | `AdditiveOnly`, `ExactHash` | Must drive compatibility classification. |
| `ResultMetadataPolicy` | `RequireBeforePayload`, `AllowStreamingUnknown` | Must govern result metadata before payload behavior. |
| `ProcedureErrorPolicy.rollback_on_error` | Boolean | Must be explicit. |
| `ProcedureErrorPolicy.allowed_error_codes` | Ordered string list | Must contain non-empty unique codes when present. |
| `MultiResultPolicy` | `SingleResultOnly`, `MultipleResultStreamsAllowed` | Must agree with result stream count. |

`required_permissions` are part of both the full contract hash and the policy
version. Their current order participates in both digests.

### Validation order

The accepted validation order for materialized contracts is:

1. Validate catalog object kind, id, name, and version.
2. Validate non-zero Procedure identity and legacy contract reference fields.
3. Validate input column descriptors.
4. Validate non-zero `StatsVersion`.
5. Validate non-zero protocol layout hashes.
6. Validate error policy.
7. Validate non-empty, unique required permissions.
8. Validate unique structured input names.
9. Validate each result stream and uniqueness of stream ids and names.
10. Validate multi-result policy against stream count.
11. Compute the canonical hash and compare it with `contract_hash`.
12. Compute and validate full `ProcedureContractBinding`.

Any failure must reject the contract before catalog publication or invocation
admission.

### Compatibility behavior

Current compatibility checks require both previous and next contracts to pass
`validate_canonical_hash()`. If either side is invalid or non-canonical, the
diagnostic is incompatible.

Compatibility also requires:

- same Procedure qualified name;
- same `ProcedureId`;
- same catalog object id;
- both catalog objects of kind `Procedure`;
- advancing `CatalogVersion` on the next contract.

For `ExactHash`, the next contract is compatible only when the contract hash is
unchanged.

For `AdditiveOnly`, the following changes are rejected:

- input changes;
- structured input changes;
- required permission changes;
- `StatsVersion` changes;
- protocol layout changes;
- transaction policy changes;
- result metadata policy changes;
- error policy changes;
- multi-result policy changes;
- removal of an existing result stream;
- change of an existing result stream id;
- change of an existing result stream cardinality;
- change to existing result stream columns except appending new columns after
  the unchanged prefix.

## Validation

Documentation acceptance checks:

- The spec states that Procedure invocation is contract-first and does not
  introduce ad hoc SQL.
- The spec uses Andromeda native terms: Procedure, Procedure contract,
  `ContractHash`, `CatalogVersion`, `StatsVersion`, `PolicyVersion`,
  `ProcedureContractBinding`, ResultStream, and StructuredObject.
- The spec marks `ProcedureContractRef` as a legacy projection and
  `ProcedureContractBinding` as the preferred full evidence.
- The spec states that `contract_hash` must equal the canonical hash.
- The spec documents the current result stream cardinality tags.
- The spec documents implemented compatibility behavior and does not invent a
  broader compatibility engine.
- The spec lists implemented and pending gaps.

Current implementation gates:

```powershell
cargo test -p andromeda-contract
cargo test -p andromeda-srpl --test compiler_pipeline_e2e
cargo test -p andromeda-srpl --test definitionbatch_compat
```

These commands are recommended for code changes that modify contract behavior.
For this documentation-only worker, Markdown and source cross-check validation
is sufficient unless repository policy requires a wider gate.

### Validation matrix

| Scenario | Expected result | Current evidence |
| --- | --- | --- |
| Candidate materializes valid Procedure contract. | Accepted with non-zero canonical `ContractHash` and full binding. | Implemented in `ProcedureContractCandidate::materialize()`. |
| Stored hash differs from canonical hash. | Rejected as `Contract` error. | Implemented by `validate_canonical_hash()` and golden tests. |
| Binding omits `StatsVersion` or `PolicyVersion`. | Rejected for full binding. | Implemented by `ProcedureContractBinding::validate()`. |
| Result stream legacy row-count flag drifts from cardinality. | Rejected as `Contract` error. | Implemented in `ResultStreamContract::validate()`. |
| Required permissions are empty or duplicate. | Rejected as `Security` error. | Implemented in `ProcedureContract::validate()`. |
| Equivalent SRPL formatting compiles to same contract hash. | Accepted with equal canonical contract hash. | Covered by SRPL DefinitionBatch source-digest tests. |
| Exact source text changes but canonical typed shape is unchanged. | Source digest changes; contract hash remains stable. | Covered by SRPL DefinitionBatch source-digest tests. |
| Additive compatibility removes an existing stream. | Incompatible diagnostic. | Implemented in `diagnose_procedure_contract_compatibility()`. |
| Application request starts transaction after contract rejection. | Must be rejected before transaction creation. | Covered outside this crate by execution and admission gates; still a cross-engine risk. |

## Current Implementation Status

Implemented:

- `andromeda-contract` owns contract descriptors, names, dependencies, catalog
  object descriptors, materialization, canonical hashing, and compatibility
  diagnostics.
- `ProcedureContractCandidate::materialize()` computes the canonical hash and
  validates the full binding.
- `ProcedureContract::canonical_hash()` recomputes the full contract hash.
- `ProcedureContract::policy_version()` computes a policy-only digest.
- `ProcedureContract::validated_binding()` validates
  `ProcedureContractBinding`.
- `ContractHash` is a 32-byte value with lowercase hex display and an all-zero
  reserved sentinel.
- SRPL lowering can materialize catalog Procedure contracts and preserve stable
  hashes across equivalent formatting.

Pending or partial:

- A dedicated compatibility crate is still planned by the roadmap; current
  compatibility is local to `andromeda-contract`.
- Full runtime propagation of `ProcedureContractBinding` across every
  execution, audit, protocol, plan-cache, and benchmark path remains a tracked
  integration gap.
- A final public binary canonicalization format is not separately versioned as
  a standalone wire or disk codec; the current hash sink is an internal stable
  implementation.
- StructuredObject payload layout and RPC payload bytes are outside this spec.
- Catalog WAL publication and crash/recovery validation are outside this spec
  and must remain covered by catalog/storage specs and tests.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Valid-looking contract is rejected as non-canonical. | A field changed after `contract_hash` was computed. | Recompute through `ProcedureContractCandidate::materialize()` or refresh the stored hash before publication. |
| Binding validation fails on policy version. | Policy fields drifted after binding evidence was produced. | Recompute `policy_version()` and rebuild the binding from the contract. |
| Result stream validation rejects a legacy flag. | `row_count_exact_required` does not match cardinality. | Derive the legacy flag from `ResultStreamCardinality`. |
| Additive compatibility rejects a change expected to be safe. | Current `AdditiveOnly` behavior is intentionally narrow. | Treat the change as incompatible or introduce a reviewed compatibility rule in a future compatibility spec. |
| Application invocation reaches transaction after contract mismatch. | Admission or execution used partial identity. | Move rejection before transaction creation and require full binding evidence. |
| Documentation says SQL signature or query text. | Non-native terminology drift. | Reword to Procedure contract, typed input shape, output ResultStream shape, and SRPL source where applicable. |

## References

- `crates/andromeda-contract/src/lib.rs`
- `crates/andromeda-contract/src/contracts/types.rs`
- `crates/andromeda-contract/src/contracts/hash.rs`
- `crates/andromeda-contract/src/contracts/materialization.rs`
- `crates/andromeda-contract/src/contracts/validation.rs`
- `crates/andromeda-contract/src/objects.rs`
- `crates/andromeda-contract/src/objects/validation.rs`
- `crates/andromeda-types/src/ids.rs`
- `crates/andromeda-types/src/types.rs`
- `crates/andromeda-contract/tests/contract_hash_golden.rs`
- `crates/andromeda-srpl/tests/compiler_pipeline_e2e/contract_hash_canonical_forms.rs`
- `crates/andromeda-srpl/tests/definitionbatch_compat/source_digest.rs`
- `documentations/specs/ContractHash_Canonicalization_v0.md`
- `documentations/specs/FrameHeader_RPC_v0.md`
- `documentations/specs/SecurityAdmission_v0.md`
