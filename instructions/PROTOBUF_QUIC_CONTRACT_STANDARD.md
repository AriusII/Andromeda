# Protobuf and QUIC Contract Standard

## Scope

Use for protocol contract work.

## Requirements

- QUIC is the transport.
- Protobuf defines contracts and payload shapes.
- gRPC must not be introduced.
- Metadata must precede payload.
- Result streams must declare shapes before batches.
- RowCountExact is required where the procedure contract requires exact cardinality.
- Backpressure behavior must be specified.
- Compatibility must be versioned.

## Required outputs

- `.proto` contract.
- QUIC frame mapping.
- Versioning and compatibility policy.
- Failure semantics.
- Observability fields.
