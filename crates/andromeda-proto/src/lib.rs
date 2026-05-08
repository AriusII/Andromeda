#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Protocol

Wire protocol types and codec utilities for RPC communication.

## Overview

The protocol module defines the on-the-wire message types used for client-server
communication with the Andromeda database engine. It provides:
- **Frame Envelopes**: Protocol wrappers with contract binding and identifiers
- **RPC Streams**: Validated sequences of metadata, batches, and completions
- **Error Messages**: Structured error responses with retry hints
- **Payload Types**: Discriminator types for different message categories

## Core Concepts

### Frame Envelopes

`FrameEnvelope` wraps a serialized payload with essential metadata:
- `protocol_version`: Locks protocol version for compatibility
- `contract_hash`: Binds message to a procedure contract
- `catalog_version`: References the catalog version at request time
- `request_id`, `session_id`, `tx_id`: Correlation and transaction IDs
- `payload_kind`: Indicates the message type
- `payload`: Serialized data (typically protobuf)

### RPC Streams

A complete RPC result stream consists of:
1. **RpcMetadata**: Result schema and streaming mode
2. **RpcBatch*** : Zero or more data batches
3. **RpcCompletion**: Summary and transaction outcome

`FrameEnvelope::validate_rpc_stream_sequence()` enforces ordering constraints.

### Error Handling

`ErrorEnvelope` provides:
- Error family classification (Protocol, Authentication, Transaction, etc.)
- Transaction effect (NoTransaction, RollbackRequired, FailStop)
- Retry disposition (NotRetryable, Retryable, RetryAfter, Backpressure)
- Backpressure hints for load shedding

### Protocol Versions

Protocol version locking ensures:
- Client and server speak the same wire format
- No mid-stream format changes
- Clean upgrade paths between versions

## Modules

| Module | Purpose |
|--------|---------|
| `envelope_frame` | Frame envelope and basic operations |
| `envelope_validation` | RPC stream sequence validation |
| `errors` | Error types and retry policies |
| `completion` | RPC completion and transaction outcome |
| `version` | Protocol version management |
| `generated` | Generated protobuf code and descriptors |
| `payload` | Payload kind discriminators |
| `structured` | Structured data types |
| `manifest` | Protocol manifest and metadata |

## Schema Evolution and Versioning

### Version Locking (V1.0)

The protocol is currently locked at **V1.0** (matching Andromeda V0.5 release).
Future changes follow this policy:

- **Major version bump:** Breaking wire format changes (e.g., field removal)
  - V1.x clients REJECT responses with major > 1
  - Requires explicit client upgrade
- **Minor version bump:** Backward-compatible extensions (e.g., new optional field)
  - V1.0 clients ignore unknown fields (proto3 default)
  - V1.1 servers accept V1.0 clients (server ignores their lack of new field)

**Invariant:** Every frame carries `protocol_version` in Field 1 (FrameEnvelope).
Version MUST be extracted before payload deserialization.

### Compatibility Matrix

| Client | Server V1.0 | Server V2.0 | Outcome |
|--------|---|---|---|
| V1.0 (current) | ✅ | ✅ Backward-compatible | Accept |
| V1.1 (future) | ✅ Backward-compatible | ✅ | Accept |
| V2.0 (future) | ❌ | ✅ | Reject or upgrade |

### Result Stream Metadata Contract

**Doctrine: Metadata-Before-Payload**

All row count information rides in frame headers (RpcMetadata, RpcBatch, RpcCompletion),
never derived from payload inspection.

- `RpcMetadata` declares schema and row count policy (EXACT_REQUIRED, EXACT_IF_KNOWN, etc.)
- `RpcBatch` includes `row_count_exact` (total rows in stream, if known upfront)
- `RpcCompletion.ResultRowCountSummary` confirms final row counts
- `row_count_exact` is idempotent across all batches (same value or absent)

### Error Classification

Every error carries:
- `family`: ErrorFamily (Protocol, Authentication, Authorization, Contract, etc.)
- `retry_disposition`: RetryDisposition (NotRetryable, Retryable, RetryAfter, Backpressure)
- `transaction_effect`: TransactionEffect (NoTransaction, RollbackRequired, FailStop)

Example error mapping (AndromedaError → ErrorEnvelope):

```text
AndromedaErrorKind::Protocol
  → ErrorFamily::Protocol + RetryDisposition::NotRetryable

AndromedaErrorKind::Exhausted
  → ErrorFamily::Resource + RetryDisposition::Backpressure
  → includes BackpressureMetadata { shed_load: true, capacity_percent: 85 }

AndromedaErrorKind::Unauthorized
  → ErrorFamily::Authorization + RetryDisposition::NotRetryable
```

See `docs/decisions/DEC-021-protobuf-schema-contract.md` for complete error taxonomy.

### Deterministic Serialization

All Protobuf messages serialize deterministically via `prost`:
- Identical input always produces identical bytes
- Enables caching, deduplication, checksumming
- No randomization or state-dependent encoding

### Malformed Input Handling

Codec functions NEVER panic:
- Truncated bytes → `Err(AndromedaError::Protocol)`
- Invalid field tags → `Err(AndromedaError::Protocol)`
- Unknown enum codes → Type-safe rejection or default variant

## Safety

This crate forbids unsafe code (`#![forbid(unsafe_code)]`).

## Examples

### Creating an Execute Request

```ignore
use andromeda_proto::FrameEnvelope;
use andromeda_types::{ContractHash, CatalogVersion, RequestId, SessionId};

let envelope = FrameEnvelope::rpc_execute_request(
    ContractHash::test_vector(1),
    CatalogVersion::new(1),
    RequestId::new(100),
    SessionId::new(200),
    None,
    b"parameter_data".to_vec(),
)?;
```

### Validating an RPC Stream

```ignore
use andromeda_proto::{FrameEnvelope, RpcResultStreamMetadataPolicy};

FrameEnvelope::validate_rpc_stream_sequence_with_metadata_policy(
    &[metadata, batch, completion],
    RpcResultStreamMetadataPolicy::RowBatchRequired,
)?;
```

"#]

mod completion;
mod errors;
pub mod generated;
mod generated_validation;
mod manifest;
mod structured;

pub use andromeda_rpc_protocol::{
    AUTH_WIRE_CODE, CONTRACT_REQUEST_WIRE_CODE, CONTRACT_RESPONSE_WIRE_CODE, ERROR_WIRE_CODE,
    FrameEnvelope, HELLO_WIRE_CODE, PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP, PayloadFrameFamily,
    PayloadFrameMapping, PayloadKind, ProtocolVersion, RPC_BATCH_WIRE_CODE,
    RPC_COMPLETION_WIRE_CODE, RPC_EXECUTE_REQUEST_WIRE_CODE, RPC_METADATA_WIRE_CODE,
    RpcResultStreamMetadataPolicy,
};
pub use andromeda_proto_wire::{
    CONTRACT_PACKAGE, DESCRIPTOR_SET_HASH_ALGORITHM, PROTOCOL_FRAME_ENVELOPE_TYPE,
    PROTOCOL_PACKAGE,
};
pub use completion::*;
pub use errors::*;
pub use generated::{
    decode_generated_message, descriptor_set_bytes, descriptor_set_hash,
    encode_generated_message, frame_envelope_hash, project_generated_frame_envelope,
    project_generated_structured_object_header, protocol_layout,
    validate_catalog_procedure_manifest_resolution_request,
    validate_catalog_procedure_manifest_resolution_response, validate_generated_error_envelope,
    validate_generated_frame_envelope, validate_generated_invocation_request,
    validate_generated_invocation_response, validate_generated_invocation_response_sequence,
    validate_generated_rpc_batch, validate_generated_rpc_completion,
    validate_generated_rpc_execute_request, validate_generated_rpc_metadata,
    validate_generated_structured_object_header,
};
pub use manifest::*;
pub use structured::*;
