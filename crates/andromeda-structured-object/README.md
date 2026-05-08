# andromeda-structured-object

## Purpose

`andromeda-structured-object` owns contract-safe StructuredObject metadata: headers, layout descriptors, row-count policy, payload bounds, descriptor hashing, and validation before payload bytes are admitted.

StructuredObject uses explicit contracts. Consumers must validate metadata before reading or forwarding payload bytes, and descriptor identity must be deterministic and tamper-evident.

## Scope

This crate provides:

- `StructuredObjectHeader` as the metadata-before-payload contract.
- `StructuredObjectLayout` values for row-major, column-major, and hybrid payload layouts.
- `RowCountRequirement` and `RowCountPolicy` for exact or bounded row-count metadata.
- Deterministic descriptor hashing over ordered field descriptors and layout.
- Validation for header identity, contract hash, descriptor hash, dense ordinals, unique field names, row-count coherence, payload length, and payload bounds.

## Non-goals

- Do not own Protobuf schemas, generated wire types, RPC framing, execution queues, catalog storage, WAL codecs, or payload serialization.
- Do not serialize Rust native structs directly to disk or network.
- Do not infer row counts from partial payload scans when exact row-count metadata is required.
- Do not accept shape-shifting returns, implicit null semantics, or payload bytes without a validated header.
- Do not use the StructuredObject name as descriptor identity. Identical shapes under different names must share descriptor identity.

## Prerequisites

- Build field descriptors with dense zero-based ordinals.
- Use unique field names and validated scalar types.
- Compute the descriptor hash from the field descriptors and layout before constructing a header.
- Provide a non-zero owning `ContractHash`.
- Declare row-count policy and payload bounds before payload admission.

## Procedure

1. Define the ordered `ColumnDescriptor` set for the payload shape.
2. Select a `StructuredObjectLayout`.
3. Compute `StructuredObjectHeader::compute_descriptor_hash(&fields, layout)`.
4. Construct `StructuredObjectHeader` with the contract hash, descriptor hash, row-count policy, payload length, and optional payload checksum.
5. Call `StructuredObjectHeader::validate()` before accepting, framing, forwarding, or storing payload bytes.
6. Reject payloads whose metadata, row-count policy, descriptor hash, or length bounds do not validate.

## Validation

For StructuredObject contract changes, use:

```powershell
cargo test -p andromeda-structured-object -- --nocapture
```

Before accepting source changes, use the broader workspace gates listed in `crates/README.md`.

## Troubleshooting

- If validation reports a descriptor hash mismatch, recompute the hash from the exact field order and layout used by the header.
- If validation rejects field ordinals, ensure ordinals are dense and zero-based.
- If `ExactRequired` fails, provide `row_count_exact`.
- If a zero-row object advertises a non-empty payload, fix the metadata or reject the payload before forwarding.
- If a payload exceeds `max_payload_length`, treat the envelope as invalid even when the checksum is present.

## References

- [Workspace crate rules](../README.md)
- [`src/lib.rs`](src/lib.rs)
- [`src/hash.rs`](src/hash.rs)
