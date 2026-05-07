# DEC-021: Protobuf Schema Contract for Frame and Result Stream Compatibility

**Date:** 2026-02-01  
**Authors:** Protobuf Contract Architect  
**Status:** ACCEPTED  
**Category:** Architecture / Schema Governance  
**Related:** D6-PROTO-COMPAT-TESTS-V0.5, DEC-017 (QUIC Runtime), DEC-014 (Module Structure)

---

## Executive Summary

This decision record establishes the Protobuf schema evolution policy, metadata contract, and error classification taxonomy for Andromeda V0.5 and beyond. It ensures that:

- **Schema versions are queryable** from any serialized frame (no side-band lookup required)
- **Backward/forward compatibility matrix is documented** and enforced (V0.5 ↔ V1.0, V1.0 ↔ V2.0)
- **Result stream metadata** (row counts, LSN correlation, completion signals) rides in frame headers, not payload
- **Error mapping is bidirectional** (AndromedaErrorKind ↔ ErrorFamily + RetryDisposition)
- **Serialization is deterministic** (identical input → identical bytes for caching/deduplication)
- **Malformed input produces typed errors**, never panics (no unsafe code in codec paths)

---

## Problem Statement

### Current State
- Frame envelopes define ProtocolVersion, but no versioning policy governs future evolution
- Result stream row counts may appear in RpcBatch or RpcCompletion; no canonical location specified
- Error handling routes protocol errors through bare AndromedaError; no recovery hints available to callers
- Test coverage for round-trip serialization is minimal; no schema compatibility matrix
- Upgrade path from V0.5 to V1.0 is undefined

### Risks Without This Policy
1. **Silent incompatibility:** Client sends V0.5 frame to V1.0 server; frame accepted but misinterpreted
2. **Data loss:** Row count exact serialized in optional field; server omits when unknown
3. **Retry storms:** Client cannot distinguish transient (backpressure, retry-after) from fatal errors
4. **Schema drift:** Proto enums evolve without versioning discipline; new codes don't work on old clients
5. **Debug burden:** No canonical way to extract protocol version from a serialized blob

---

## Proposed Policy

### 1. Schema Versioning (Major, Minor)

#### 1.1 Protocol Version Structure
```protobuf
message ProtocolVersion {
  uint32 major = 1;  // Breaking changes only
  uint32 minor = 2;  // Backward-compatible extensions
}
```

**Rules:**
- `major` increments only when the wire format is incompatible (e.g., FrameEnvelope field removed)
- `minor` increments for backward-compatible additions (e.g., new optional field in RpcBatch)
- Current locked version: **V1.0** (matches Andromeda V0.5 shipping)
- Server MUST reject `major > SUPPORTED_MAJOR` (future-proofing)
- Server MUST accept `major == SUPPORTED_MAJOR` and `minor <= SUPPORTED_MINOR` (backward compat)

#### 1.2 Version Queryability
- Every frame envelope carries a `protocol_version` field (Field 1 in FrameEnvelope proto)
- Clients MUST extract version from serialized frame before attempting further deserialization
- No side-band metadata or content-type headers; version lives in the message itself
- Enables transparent protocol upgrade: client can sniff version without full deserialization

#### 1.3 Compatibility Matrix

| Client Version | Server V1.0 | Server V2.0 | Action |
|---|---|---|---|
| V1.0 (current) | ✅ Full | ✅ Backward-compatible | Send |
| V1.1 (future minor) | ✅ Backward-compatible | ✅ Full | Send |
| V2.0 (future major) | ❌ Reject | ✅ Full | Negotiate or fail |
| V0.5 (past) | ❌ Reject | ❌ Reject | Unsupported |

**Interpretation:**
- Client and server MUST agree on protocol major version
- Server accepts client frames with `minor <= server.minor` (server has more features)
- Server rejects frames with `major > server.major` (too new)
- Client rejects responses with `major != negotiated_major` (version skew)

#### 1.4 Version Negotiation Flow
1. Client sends HELLO frame with V1.0
2. Server responds with HELLO + offered protocol version
3. If versions differ, fail the handshake
4. All subsequent frames in session use negotiated version
5. No in-stream version changes allowed

---

### 2. Result Stream Metadata Contract

#### 2.1 Row Count Policy

**Doctrine: Metadata-Before-Payload**

All row count information MUST be serialized in frame headers, before payload bytes, to enable early validation and resource allocation decisions.

##### Field Semantics
| Field | Message | Type | Meaning |
|---|---|---|---|
| `rows_emitted` | RpcBatch | uint64 | Rows included in this batch's payload |
| `row_count_exact` | RpcBatch | optional uint64 | Total rows in entire result stream (if known) |
| `row_count_exact` | RpcCompletion.ResultRowCountSummary | optional uint64 | Final row count after all batches |
| `row_count_requirement` | ResultStreamDescriptor | enum | Policy: UNKNOWN_ALLOWED, EXACT_IF_KNOWN, EXACT_REQUIRED |

##### Encoding Rules
1. `rows_emitted` MUST equal the count of logical rows in `structured_payload`
2. `row_count_exact` (when present) MUST be ≥ `rows_emitted` in current batch
3. `row_count_exact` MUST be idempotent across all batches (same value or absent)
4. If `row_count_requirement == EXACT_REQUIRED`, first batch MUST contain `row_count_exact`
5. If `row_count_requirement == UNKNOWN_ALLOWED`, `row_count_exact` MAY be absent

##### Examples

**Example 1: Stream with exact count known upfront**
```
RpcMetadata {
  result_streams: [
    { stream_name: "users", row_count_requirement: EXACT_REQUIRED, row_count_exact: 1000 }
  ]
}
RpcBatch { result_name: "users", batch_index: 1, rows_emitted: 100, row_count_exact: 1000 }
RpcBatch { result_name: "users", batch_index: 2, rows_emitted: 100, row_count_exact: 1000 }
...
RpcCompletion { result_row_counts: [{ result_name: "users", rows_emitted: 1000, row_count_exact: 1000 }] }
```

**Example 2: Stream with row count discovered late**
```
RpcMetadata {
  result_streams: [
    { stream_name: "results", row_count_requirement: EXACT_IF_KNOWN }
  ]
}
RpcBatch { result_name: "results", batch_index: 1, rows_emitted: 50, row_count_exact: None }
RpcBatch { result_name: "results", batch_index: 2, rows_emitted: 50, row_count_exact: Some(150) }
RpcCompletion { result_row_counts: [{ result_name: "results", rows_emitted: 150, row_count_exact: 150 }] }
```

#### 2.2 Completion Signals

**Doctrine: Terminal Status is Idempotent**

A result stream ends with exactly one RpcCompletion frame carrying:
- `status`: One of COMMITTED, ROLLED_BACK, FAILED_BEFORE_TRANSACTION, CANCELLED, POISONED, PERMISSION_DENIED, CONTRACT_REJECTED, SYSTEM_UNAVAILABLE
- `transaction_outcome`: One of NOT_STARTED, COMMITTED, ROLLED_BACK, FAILED, CANCELLED
- `durable_lsn`: Present and nonzero iff status is COMMITTED or ROLLED_BACK

**Rules:**
1. Exactly one RpcCompletion per request (no retries of completion signal)
2. If status is COMMITTED or ROLLED_BACK, `durable_lsn` MUST be present and nonzero
3. If status is FAILED_BEFORE_TRANSACTION, `transaction_outcome` MUST be NOT_STARTED
4. Multiple completion signals with different statuses are protocol violations (detected at frame validation layer)

#### 2.3 LSN (Log Sequence Number) Correlation

- Completion frame carries `durable_lsn` only for transactional terminals (COMMITTED, ROLLED_BACK)
- LSN value is monotonically increasing across all transactions on a session
- Client can use LSN to correlate durable state with application checkpoints
- LSN MUST NOT appear in non-terminal states (metadata, batches, pre-transaction errors)

---

### 3. Error Classification and Recovery Mapping

#### 3.1 Error Taxonomy

```
ErrorEnvelope {
  family: ErrorFamily (Protocol, Authentication, Authorization, Contract, Semantic, Execution, Transaction, Storage, Resource)
  retry_disposition: RetryDisposition (NotRetryable, Retryable, RetryAfter, Backpressure)
  transaction_effect: TransactionEffect (NoTransaction, RollbackRequired, FailStop)
  backpressure: optional BackpressureMetadata (for Backpressure disposition)
}
```

#### 3.2 Family Semantics

| Family | Root Cause | Example | Recovery |
|---|---|---|---|
| Protocol | Wire format violation | Unknown frame type | Upgrade client |
| Authentication | Credential failure | Invalid JWT | Reauthenticate |
| Authorization | Permission denied | Caller lacks EXECUTE | Request elevated permission |
| Contract | Procedure not found or invalid | Unknown procedure name | Refresh catalog |
| Semantic | Input validation failure | Null passed to NOT NULL column | Fix input |
| Execution | Runtime failure in procedure | Division by zero | Retry or escalate |
| Transaction | Transaction-specific error | Serialization conflict | Retry transaction |
| Storage | Disk/WAL error | I/O timeout | Retry or contact ops |
| Resource | Out of memory/connections | Buffer allocation failed | Shed load or escalate |

#### 3.3 Retry Disposition Semantics

| Disposition | Meaning | Action |
|---|---|---|
| NotRetryable | Calling again will fail identically | Log error, do not retry |
| Retryable | May succeed if retried (e.g., transient state) | Retry with exponential backoff |
| RetryAfter | Retry after specified milliseconds | Wait `retry_after_ms` then retry |
| Backpressure | System overloaded; shed load | Wait, reduce request rate, check capacity_percent |

#### 3.4 Transaction Effect Semantics

| Effect | Meaning | Guarantees |
|---|---|---|
| NoTransaction | No transaction was started | Request failed before TX began; no cleanup needed |
| RollbackRequired | Transaction was started but aborted | Automatic rollback in progress; client may checkpoint |
| FailStop | Server-side panic or corruption | Escalate to ops; session must close |

#### 3.5 Mapping Rules (AndromedaError → ErrorEnvelope)

```
AndromedaErrorKind::Protocol
  → ErrorFamily::Protocol
  → RetryDisposition::NotRetryable
  → TransactionEffect::NoTransaction

AndromedaErrorKind::Transient
  → ErrorFamily::Execution (or Storage, Transaction)
  → RetryDisposition::Retryable
  → TransactionEffect::RollbackRequired (if TX started)

AndromedaErrorKind::Exhausted
  → ErrorFamily::Resource
  → RetryDisposition::Backpressure
  → BackpressureMetadata { shed_load: true, capacity_percent: 95 }

AndromedaErrorKind::Unauthorized
  → ErrorFamily::Authorization
  → RetryDisposition::NotRetryable
  → TransactionEffect::NoTransaction

AndromedaErrorKind::NotFound
  → ErrorFamily::Contract (for procedures)
  → RetryDisposition::NotRetryable (send catalog refresh to client)
  → TransactionEffect::NoTransaction
```

#### 3.6 Backpressure Metadata

```protobuf
message BackpressureMetadata {
  optional uint64 retry_after_ms = 1;    // Wait at least this many ms
  optional uint32 capacity_percent = 2;  // 0..100 when present; 0 = reserved capacity
  bool shed_load = 3;                    // True = reject new requests until recovery
}
```

**Usage:**
- Sent when server capacity is constrained (e.g., 85% heap usage)
- Client SHOULD refuse new requests if `shed_load == true`
- Client SHOULD use `retry_after_ms` to schedule retry
- Client MAY use `capacity_percent` to adjust request intensity

---

### 4. Serialization and Determinism

#### 4.1 Deterministic Serialization Contract

- All Protobuf messages MUST serialize deterministically via `prost` codec
- No randomization, ordering, or state-dependent behavior in codec paths
- Identical input → identical bytes on every call
- Enables: caching, deduplication, checksumming, audit trails

#### 4.2 Implementation Guarantee

- Use `prost::Message::encode_to_vec()` for all frames
- No custom encoding logic
- No optional field defaults that vary by Rust version

#### 4.3 Validation in Tests

Test suite validates:
- Multiple serializations of same input produce identical bytes
- Deserialization restores all fields exactly
- Edge cases (empty, zero, max u64) preserve values

---

### 5. Error Handling (No Panics in Codec Paths)

#### 5.1 Malformed Input Handling

```rust
pub fn decode_generated_message<M>(bytes: &[u8]) -> AndromedaResult<M>
where
    M: prost::Message + Default,
{
    M::decode(bytes).map_err(|error| {
        AndromedaError::new(
            AndromedaErrorKind::Protocol,
            format!("protobuf decode failed: {error}"),
        )
    })
}
```

**Contract:**
- Truncated bytes → Err(Protocol)
- Invalid field tags → Err(Protocol)
- Invalid enum values → Err(Protocol)
- Never panic; all errors are typed and recoverable

#### 5.2 Unknown Variant Handling

- Unknown PayloadKind wire code → `PayloadKind::try_from()` returns Err
- Unknown error family code → Treated as unclassified (default to NotRetryable)
- Unknown completion status code → Treated as system error

#### 5.3 Validation Invariants

Frame validation MUST reject:
- FrameEnvelope with protocol_version.major == 0
- FrameEnvelope with protocol_version.major > SUPPORTED_MAJOR
- RpcExecuteRequest with empty procedure_name
- RpcBatch with rows_emitted > row_count_exact (if exact known)
- RpcCompletion with status COMMITTED but durable_lsn missing
- Multiple RpcCompletion frames in single stream (caught at envelope validation layer)

---

## Backward and Forward Compatibility

### V0.5 to V1.0 (Current Implementation)

**Forward Compatible:** V1.0 server accepts V0.5 client frames (no breaking changes)
**Backward Compatible:** V0.5 client rejects V1.0 server frames with major > 1 (fails safely)

- `ProtocolVersion::V1` is locked
- All reserved field ranges block future extensions without version bump
- Result stream metadata contract is V1.0-only (V0.5 stubs have no metadata policy)

### V1.0 to V1.1 (Future Minor)

Example minor version bump:
- After an explicit decision-record update narrows the reserved quarantine for
  the chosen tag, add optional field to RpcBatch:
  `transaction_id: optional uint64`
- V1.0 clients ignore unknown field (proto3 default behavior)
- V1.1 clients use field if present, ignore if absent
- Version negotiation accepts V1.1 client on V1.0 server (server ignores new field)

### V1.x to V2.0 (Future Major)

Example major version bump:
- Rename FrameEnvelope field
- Add required field to RpcCompletion
- Change PayloadKind enum values
- V1.x clients REJECT major == 2 at handshake
- Requires explicit client upgrade and new connection

---

## V1 Proto Migration, Deprecation, and Reservation Policy

This update closes the V1.0 schema-evolution gap for the crate-local authority
`crates/andromeda-proto/proto/andromeda/**`.

### Field Migration Rules

1. V1.0 field numbers, wire types, cardinality (`optional`, singular,
   `repeated`, `oneof` membership), enum numeric values, package names, and
   message names are locked.
2. A field number MUST NOT be reused for a different semantic meaning.
3. A field type MUST NOT be changed in place, even when protobuf would encode it
   with a compatible wire type. Semantic compatibility is part of the contract.
4. Moving a field between messages, changing `oneof` membership, changing enum
   numeric values, or changing metadata placement is a breaking change and
   requires a new major protocol version.
5. V1 additive fields are not opened by default. Existing `reserved` ranges are
   governance quarantine ranges; assigning from them requires an explicit
   decision-record update, a minor-version compatibility statement, generated
   descriptor regeneration, and release/catalog manifest acknowledgment.

### Deprecation Rules

1. V1.0 has no active deprecated protobuf fields.
2. A future V1.x deprecation MUST keep the field number, type, cardinality, and
   decode behavior intact until the next major version.
3. Deprecated fields MUST NOT be repurposed, aliased to a new semantic, or used
   to smuggle JSON/ad-hoc payloads.
4. A deprecation requires an explicit decision-record entry naming the message,
   field name, field number, replacement, writer behavior, reader behavior, and
   the first version that stops emitting the field.
5. Governance tests MUST enumerate any approved deprecated fields. An
   unenumerated `[deprecated = true]` option is treated as schema drift.

### Reservation Rules

1. Every message MUST declare a reserved field-number range to prevent accidental
   ad-hoc extension.
2. When a field is removed in a future major version, its field number and its
   former field name MUST be reserved forever in the replacement schema.
3. Reserved numbers and names MUST NOT be reused in the same package lineage.
4. Reserved ranges may only be narrowed by an explicit decision-record update
   that states why the new field is wire-compatible with all supported readers.

### `generated.rs` Lockstep

1. `src/generated.rs` remains the only public wrapper around prost output for
   these contracts.
2. The wrapper MUST include the generated `FileDescriptorSet` bytes from the
   build output and the generated modules for
   `andromeda.contract.v1` and `andromeda.protocol.v1`.
3. Any proto-source change and any build-pipeline change that affects generated
   prost output MUST be reviewed together. Schema source, descriptor set,
   generated Rust module layout, and public hash helpers are one lockstep unit.
4. The generated wrapper MUST preserve `descriptor_set_hash()`,
   `frame_envelope_hash()`, `protocol_layout()`, and the declared
   `DESCRIPTOR_SET_HASH_ALGORITHM` so manifests can bind to a stable protocol
   layout without importing generated implementation details.

### Descriptor Hash Governance

1. `descriptor_set_hash` is a deterministic digest over the generated descriptor
   set; `frame_envelope_hash` is a domain-separated digest over the same
   descriptor bytes for the `FrameEnvelope` contract.
2. Intentional schema changes MUST change the descriptor digest and therefore
   require catalog/release manifest acknowledgement before peers rely on the new
   layout.
3. Governance tests assert structural invariants instead of pinning inline hash
   bytes: nonzero digests, domain separation, call stability, descriptor/source
   path equality, package equality, and service-free descriptors.
4. Hash drift without a matching schema decision and manifest handoff is a
   release blocker.

### Compatibility Test Expectations

The protobuf governance tests MUST continue to cover:

- crate-local proto tree authority and descriptor/source equality;
- message-only schemas with no `service`, `rpc`, gRPC, tonic, or generated
  service surface;
- no runtime JSON policy (`serde_json` dependencies and `json_name` mapping
  options are forbidden on the protocol/result surface);
- reserved ranges on every message;
- no unrecorded deprecated fields in V1.0;
- generated wrapper lockstep with descriptor bytes and split package modules;
- deterministic serialization, round-trip preservation, malformed-input typed
  errors, field presence semantics, payload-kind discriminator lockstep, and
  metadata-before-payload row-count expectations.

---

## Integration Points

### D5: Stream Concurrency
- Per-stream completion signals MUST include request_id for correlation
- Multiple batches with same request_id belong to same request's stream
- Completion signal terminal status concludes the stream

### D4: Executor Bridge  
- Executor emits RpcBatch with `row_count_exact` from procedure result descriptor
- Executor emits RpcCompletion with `transaction_outcome` from TX state machine
- Executor MUST set `durable_lsn` iff transaction reached terminal state

### Protocol Transport (QUIC)
- Frame envelope maps to QUIC STREAM frame (reliable, ordered)
- PayloadKind wire codes are stable across all QUIC stream types
- No QUIC DATAGRAM carries contract-bound RPC payloads (metadata-before-payload doctrine)

---

## Test Coverage

**Test Suite:** `crates/andromeda-proto/tests/frame_result_compat.rs`

| Test | Scenario | Validates |
|---|---|---|
| `test_frame_header_protobuf_round_trip` | Basic serialization | No data loss |
| `test_frame_payload_serialization_deterministic` | Multiple encodings | Identical bytes |
| `test_result_stream_metadata_round_trip` | Schema descriptors | Metadata preserved |
| `test_row_count_exact_encoding` | Edge cases (0, max u64) | Row count semantics |
| `test_completion_signal_variants` | All status codes | Terminal status enum |
| `test_error_envelope_preservation` | Full error details | Error mapping preserved |
| `test_schema_version_tag_present_in_frame` | Version queryability | Version in header |
| `test_malformed_protobuf_typed_error` | Truncated/invalid bytes | Typed error, no panic |
| `test_partial_read_backpressure_signal` | Load shedding | Backpressure semantics |
| `test_multiple_completion_signals_rejected` | Stream ordering | Completion idempotence |
| `test_rpc_batch_field_validation` | Batch fields | Batch schema |
| `test_protocol_version_validation` | Version rules | Version locking |
| `test_payload_kind_discriminator_lockstep` | Wire codes | PayloadKind codes locked |

---

## Anti-Patterns and Rejections

### ❌ Rejected: Embedding version in payload
- Version MUST be in frame header, not inside serialized payload
- Rationale: Server must validate version before payload deserialization

### ❌ Rejected: Side-band metadata (HTTP headers, DNS)
- Protocol version MUST be queryable from serialized bytes alone
- Rationale: QUIC frames are standalone; no co-transport metadata available

### ❌ Rejected: Variable-length encoding for row counts
- Row counts use fixed uint64; no zigzag or varint tricks
- Rationale: Enables schema stability for future minor versions

### ❌ Rejected: Implicit version negotiation
- HELLO frame MUST include explicit protocol_version
- Rationale: Failure to agree is fatal; no silent downgrade

### ❌ Rejected: Backward compatibility for major version changes
- V0.5 → V1.0 allowed; V1.0 → V2.0 requires client upgrade
- Rationale: Major versions are breaking by definition

---

## Open Questions and Future Work

1. **V1.1 Preview Field Tagging:** How to mark proto fields as preview/preview-only?
   - *Defer to next iteration; current V1.0 locks all fields*

2. **LSN Checkpointing:** Should clients durably store LSN?
   - *Guidance: yes, for recovery; encoding via catalog snapshots deferred to D7*

3. **Error Code Registry:** Central registry for error codes across procedures?
   - *Defer; currently using free-form strings; may require contract layer in V2.0*

4. **Compression:** Should payload be gzip-compressed in frames?
   - *Defer; QUIC handles compression at transport layer*

---

## Acceptance Criteria

- ✅ All 12+ tests passing
- ✅ Round-trip serialization deterministic
- ✅ Malformed input produces typed error (no panic)
- ✅ Decision record specifies compatibility policy
- ✅ Schema version tags queryable from frames
- ✅ No gRPC, pure Protobuf + custom frame wrapper
- ✅ V1 migration/deprecation/reservation policy defined and governed by tests
- ✅ `generated.rs`, descriptor hash, and compatibility-test lockstep documented
- ✅ No unsafe code in serialization paths
- ✅ Error mapping documented (AndromedaErrorKind ↔ ErrorFamily)
- ✅ Backward compatibility matrix in place (V0.5 ↔ V1.0 ↔ V2.0 plans)
- ✅ Result stream metadata contract specified (row counts, LSN, completion)

---

## Conclusion

This decision record establishes durable schema governance for Andromeda wire protocol. By locking version discipline, metadata placement, and error classification, we enable:

1. **Safe protocol evolution** without breaking existing clients
2. **Predictable error recovery** (clients know action for each error family)
3. **Deterministic serialization** (auditable, cacheable)
4. **Clear upgrade paths** (V0.5 → V1.0 → V2.0 with explicit handoff)
5. **Type-safe error handling** (no panics in codec paths)

The schema is production-locked as of 2026-02-01 and MUST NOT change without a new decision record.
