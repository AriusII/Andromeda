# Protobuf and QUIC Contract Standard

## Scope

Use for protocol contract work.

## Requirements

- QUIC is the transport.
- Protobuf defines contracts and payload shapes (prost 0.14+).
- gRPC must not be introduced.
- **Metadata-Before-Payload Doctrine:** All row counts, LSN correlation, and completion signals MUST be serialized in frame headers or metadata before payload bytes.
- Result streams must declare shapes and metadata before batches.
- `RowCountExact` is required where the procedure contract requires exact cardinality; it must be sent before payload streaming.
- Backpressure behavior must be specified.
- **Versioning (DEC-021):** Every frame envelope must carry a `ProtocolVersion` (major, minor).
    - Current locked version: **V1.0**.
    - Server MUST reject `major > SUPPORTED_MAJOR`.
    - Server MUST accept `major == SUPPORTED_MAJOR` and `minor <= SUPPORTED_MINOR`.
- Catalog manifest resolution failures MUST use typed protobuf statuses, not JSON:
    - unsupported, malformed, internal, catalog-not-ready, auth-required, permission-denied, not-found, version-mismatch, hash-mismatch, and source-generator-not-ready.

## Required outputs

- `.proto` contract.
- QUIC frame mapping.
- Versioning and compatibility policy.
- Failure semantics.
- Observability fields.
