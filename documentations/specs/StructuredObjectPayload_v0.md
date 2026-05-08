# StructuredObjectPayload v0 Specification

## Purpose

Define the accepted contract for admitting `StructuredObject` payloads through
metadata-first validation.

`StructuredObjectPayload v0` treats the header as the authoritative contract
evidence for shape, type descriptors, row-count policy, descriptor hashing, and
payload byte bounds before any payload bytes are interpreted. It is a contract
admission specification, not a native Rust serialization format.

## Scope

This specification applies to the current `andromeda-structured-object` and
`andromeda-types` responsibilities:

- `StructuredObjectHeader` metadata validation;
- field validation through `ColumnDescriptor` and `TypeDescriptor`;
- explicit absence policy carried by each field;
- decimal precision and scale bounds;
- text encoding, maximum length, and collation bounds;
- descriptor-hash verification for field shape and layout;
- payload length, maximum payload length, and row-count coherence.

The specification covers the metadata gate that a caller must pass before a
payload is accepted or forwarded. Payload bytes remain opaque to this crate
until a separate codec or wire specification defines their physical encoding.

## Non-goals

This specification does not:

- introduce application-facing ad hoc SQL or dynamic predicates;
- bypass typed Procedure contracts or `ContractHash` evidence;
- authorize implicit null semantics or SQL-style three-valued logic;
- serialize Rust native structs directly to disk or network;
- define a Protobuf, QUIC, WAL, page, or manifest byte layout;
- define a JSON runtime payload format;
- place GPU, benchmark, learned, or transient RAM output in a validation path
  that decides payload truth;
- decide catalog publication, WAL durability, rollback, recovery, or IAM
  authorization behavior.

## Prerequisites

Reviewers must use these references before accepting changes to this contract:

- `AGENTS.md` for Andromeda non-negotiable invariants.
- `crates/AGENTS.md` for crate boundary rules.
- `documentations/specs/TypeSystem_v0.md` for scalar, absence, decimal, float,
  text, and StructuredObject ties.
- `documentations/specs/ProcedureContract_v0.md` for metadata-before-payload
  and Procedure contract boundaries.
- `documentations/specs/ContractHash_Canonicalization_v0.md` for hash newtype
  behavior and scalar tag precedent.
- `crates/andromeda-types/src/types.rs` for current type validation.
- `crates/andromeda-types/src/ids.rs` for `ContractHash` rules.
- `crates/andromeda-structured-object/src/lib.rs` for header validation.
- `crates/andromeda-structured-object/src/hash.rs` for descriptor hashing.

## Procedure

### Header admission

A consumer must validate the complete `StructuredObjectHeader` before
interpreting or forwarding the payload bytes.

The header is valid only when all of the following conditions hold:

- `name` is not empty after trimming.
- `contract_hash` is nonzero.
- `descriptor_hash` is nonzero.
- `fields` contains at least one field.
- `column_count` equals `fields.len()`.
- each field validates through `ColumnDescriptor::validate()`;
- field ordinals are dense and zero-based;
- field names are unique within the header;
- the recomputed descriptor hash equals `descriptor_hash`;
- `ExactRequired` row-count policy carries `row_count_exact`;
- declared row-count evidence and `payload_length` are coherent;
- `payload_length` does not exceed `max_payload_length` when a maximum is
  declared.

Rejecting a header at this stage must prevent payload interpretation.

### Type and absence validation

Every payload field is described by:

```text
ColumnDescriptor = name + TypeDescriptor + ordinal
TypeDescriptor = ScalarType + AbsencePolicy
```

`AbsencePolicy` is part of the payload contract. `Required` means the payload
must contain a value for that field. `ExplicitOptional` means absence is an
explicit branch that the caller and consumer must handle. Absence is not an
ambient `NULL`, a third boolean truth value, or an untyped payload sentinel.

Diagnostic projections may expose a nullable field for observability, but
runtime validation must map that projection back to `AbsencePolicy`.

### Decimal validation

Decimals are exact numeric shapes. A custom decimal field must satisfy:

- `precision > 0`;
- `scale <= precision`;
- any precision or scale change is descriptor-hash relevant and
  compatibility-relevant.

Invalid decimal descriptors fail the contract gate before payload admission.

### Text validation

Text fields must carry explicit encoding. Current accepted encodings are
`Utf8`, `Utf16`, and `Unicode`.

Text validation requires:

- `max_length`, when present, is greater than zero;
- `collation`, when present, is not empty after trimming;
- unbounded text remains subject to the owning Procedure, RPC, resource, and
  payload bounds;
- equality, ordering, uniqueness, and indexing policies that depend on text
  comparison must declare their comparison policy outside the payload bytes.

Invalid text bounds fail the contract gate before payload admission.

### Descriptor hash

The descriptor hash is the tamper-evident identity for the typed payload shape.
It must be recomputed from the metadata and compared with the declared
`descriptor_hash`.

The current descriptor hash input includes:

- an explicit StructuredObject descriptor domain tag;
- layout tag;
- field count;
- each field name;
- each field ordinal;
- scalar family and scalar parameters;
- decimal precision and scale when custom;
- float width and mode when custom;
- text encoding, maximum length presence and value, and collation presence and
  value;
- timestamp subtype;
- absence policy.

The descriptor hash excludes the `StructuredObject` name. Two differently
named payloads with identical field shape and layout may share a descriptor
hash. Any field name, ordinal, scalar, scalar parameter, absence policy, or
layout change must change the descriptor hash.

### Row-count and payload bounds

Row-count metadata is contract evidence, not a value inferred from a partial
payload scan.

The current row-count rules are:

- `ExactRequired` requires `row_count_exact`.
- `row_count_exact = Some(0)` requires `payload_length = 0`.
- `row_count_exact = Some(n)` where `n > 0` requires `payload_length > 0`.
- `UnknownAllowed` may omit `row_count_exact`.
- `payload_length` must be less than or equal to `max_payload_length` when a
  maximum is declared.

`payload_checksum` is optional opaque evidence at this layer. It does not
replace descriptor-hash validation, row-count validation, or byte-bound checks.

### Error classification

Current implementation classifies shape and type descriptor failures as
`Contract` errors. Payload envelope coherence failures, such as row-count to
payload-length mismatch or payload length exceeding a declared bound, are
`Protocol` errors.

## Validation

Use targeted Rust gates for changes in this scope:

```powershell
cargo test -p andromeda-types --lib
cargo test -p andromeda-structured-object --lib
```

For wider changes that affect Procedure contracts, RPC framing, catalog
publication, or payload codecs, add the owning crate tests and any relevant
property, fuzz, compatibility, or crash/recovery validation.

Acceptance checks:

| Scenario | Expected result |
| --- | --- |
| Required versus explicit optional field differs only by absence policy. | Descriptor hash changes. |
| Custom decimal precision or scale changes. | Descriptor hash changes. |
| Custom decimal has zero precision or `scale > precision`. | Header validation rejects the payload as `Contract`. |
| Text max length is `Some(0)`. | Header validation rejects the payload as `Contract`. |
| Text collation is present but empty. | Header validation rejects the payload as `Contract`. |
| Field shape changes after `descriptor_hash` is computed. | Header validation rejects the payload as `Contract`. |
| Zero rows declare a non-empty payload. | Header validation rejects the payload as `Protocol`. |
| Positive exact row count declares an empty payload. | Header validation rejects the payload as `Protocol`. |
| Payload length exceeds declared maximum. | Header validation rejects the payload as `Protocol`. |

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Payload is rejected with a descriptor-hash mismatch. | The fields, scalar parameters, absence policy, or layout changed after the hash was computed. | Recompute the descriptor hash from the final metadata before publication. |
| Optional data behaves like SQL `NULL`. | A diagnostic or foreign projection was treated as runtime authority. | Map it back to `AbsencePolicy` and handle the absent branch explicitly. |
| Decimal payload values are accepted for an invalid shape. | The caller skipped `TypeDescriptor::validate()` through header validation. | Validate the header before accepting payload bytes. |
| Text equality behavior is ambiguous. | Collation or comparison policy was omitted by the owning contract. | Add explicit comparison policy at the Procedure, catalog, or consumer boundary. |
| Empty payload is rejected even though streaming is expected. | The header declares a positive exact row count with zero payload bytes. | Use coherent row-count metadata or omit exact row count only when policy allows unknown counts. |
| A checksum mismatch is treated as a descriptor mismatch. | Content checksum evidence and descriptor hash evidence were conflated. | Validate descriptor shape separately from payload content evidence. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `documentations/specs/TypeSystem_v0.md`
- `documentations/specs/ProcedureContract_v0.md`
- `documentations/specs/ContractHash_Canonicalization_v0.md`
- `crates/andromeda-types/src/types.rs`
- `crates/andromeda-types/src/ids.rs`
- `crates/andromeda-structured-object/src/lib.rs`
- `crates/andromeda-structured-object/src/hash.rs`
- `crates/andromeda-structured-object/src/tests.rs`
