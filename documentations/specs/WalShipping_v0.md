# WAL Shipping v0 Specification

## Purpose

Define the accepted v0 contract for single-primary WAL shipping in Andromeda
HA/DR. WAL shipping moves only durable WAL evidence from the active primary to
replicas and produces bounded evidence that quorum, fencing, retention,
promotion, recovery, and operator diagnostics can consume.

WAL shipping is not database truth by itself. The source of truth remains
durable WAL plus validated storage, catalog, manifest, and recovery evidence.
Shipping must not make a commit visible before durable WAL, must not trust RAM
as truth, and must not serialize Rust native structs directly to disk or
network.

## Scope

This specification applies to v0 WAL shipping contracts:

- primary-to-replica role boundaries;
- durable source LSN and shippable segment conditions;
- shipping segment descriptors and envelopes;
- contiguous `WalShipmentBatch` validation;
- replica expectation and backpressure coordinates;
- replica durable ACKs and safe-LSN tracking;
- retention blocking when required replicas lag;
- fencing events caused by disconnects, checksum mismatch, LSN gaps, or
  unclassified failures;
- validation evidence required before a shipped range can participate in
  promotion or retention decisions.

It covers current Rust surfaces in:

- `crates/andromeda-storage/src/hadr/shipping_runtime.rs`
- `crates/andromeda-storage/src/hadr/shipping_runtime/segment.rs`
- `crates/andromeda-storage/src/hadr/shipping_runtime/flow_control.rs`
- `crates/andromeda-storage/src/hadr/shipping_runtime/fencing.rs`
- `crates/andromeda-storage/src/write_ahead_log/shipping.rs`
- `crates/andromeda-storage/src/write_ahead_log/shipping/batch.rs`
- `crates/andromeda-storage/src/write_ahead_log/shipping/ack.rs`
- `crates/andromeda-storage/src/write_ahead_log/segment_reclaimability/`

## Current Implementation Status

The repository contains runtime-free WAL shipping contracts and tests for
segment structure, checksum validation, chain validation, replica expectations,
durable ACK tracking, and segment reclaimability. This specification does not
claim that QUIC streaming, retry scheduling, persistent multi-segment archives,
or end-to-end replica catchup are release-ready.

Implemented evidence currently includes:

- `ShippingCondition`;
- `ShippingBackpressureRequest`;
- `ShippingSegmentDescriptor`;
- `ShippingSegmentEnvelope`;
- `WalNodeIdentity` and `WalNodeRole`;
- `WalReplicaExpectation`;
- `WalShipmentBatch`;
- `WalShipmentAccepted` and `WalShipmentRange`;
- `WalShippingAck`;
- `WalReplicaSafeLsnTracker`;
- reclaimability blocking through required replica safe LSNs.

## Non-goals

This specification does not:

- define the WAL record byte format; see `WalRecord_v0.md`;
- define QUIC frame bytes, stream scheduling, or retry policy;
- introduce gRPC, runtime JSON defaults, ad hoc SQL, or dynamic command text;
- bypass Procedure contracts or surface HA/DR operations through the
  Application surface;
- authorize shipping of records not yet durable at the primary;
- authorize replica ACKs before durable replica receipt;
- serialize Rust native structs directly to disk or network;
- treat shipping traces, metrics, RAM state, GPU output, or benchmark output as
  durable truth;
- define full PITR archive retention or restore orchestration.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `documentations/specs/WalRecord_v0.md`
- `documentations/specs/HadrQuorumFencing_v0.md`
- `documentations/specs/RecoveryReport_v0.md`
- `documentations/governance/decisions/DEC-019-f1-wal-shipping-runtime.md`
- `documentations/governance/decisions/DEC-020-quorum-runtime.md`
- `crates/andromeda-storage/src/hadr/shipping_runtime.rs`
- `crates/andromeda-storage/src/hadr/shipping_runtime/segment.rs`
- `crates/andromeda-storage/src/hadr/shipping_runtime/flow_control.rs`
- `crates/andromeda-storage/src/write_ahead_log/shipping.rs`
- `crates/andromeda-storage/src/write_ahead_log/shipping/batch.rs`
- `crates/andromeda-storage/src/write_ahead_log/shipping/ack.rs`
- `crates/andromeda-storage/tests/wal_shipping_reclaimability_contract.rs`
- `crates/andromeda-storage/tests/hadr_quorum_contract.rs`

## Procedure

### Role boundary

WAL shipping is single-primary in v0.

| Role | Allowed behavior |
| --- | --- |
| `Primary` | Reads durable WAL ranges and ships them to replicas. |
| `Replica` | Receives, validates, durably appends, and ACKs contiguous WAL ranges. |

The shipping contract rejects a shipment from a non-primary source and rejects a
shipment to a non-replica target. A node cannot silently change role within a
shipment. HADR role changes must go through quorum and fencing.

### Durable source condition

A WAL segment or range is shippable only when:

```text
segment_end_lsn <= primary_durable_lsn
```

The shipping thread may cache the primary durable LSN, but it must treat the
durable WAL source as truth. It must not ship records that only exist in a
buffer, queue, transaction-local state, or temporary file.

Shipping a record does not make it visible. Visibility remains governed by
transaction commit, WAL durability, quorum write admission, and fencing.

### Shipping descriptor and envelope

A shipping segment descriptor carries bounded metadata:

| Field | Rule |
| --- | --- |
| `segment_id` | Monotonic segment identity within the shipping source. |
| `start_lsn` | First LSN in the shipped segment, inclusive. |
| `end_lsn` | Last LSN in the shipped segment, inclusive. |
| `record_count` | Exact number of WAL records in the envelope. |
| `checksum` | Deterministic checksum over the shipped record bytes. |

The envelope carries the descriptor and borrowed records. The envelope must pass
structure validation and checksum validation before it is sent or accepted as
replica input.

Envelope validation rejects:

- record-count mismatch;
- empty record set;
- invalid LSN range;
- first-record LSN mismatch;
- last-record LSN mismatch;
- checksum mismatch.

The envelope is not the durable wire format. Network and disk persistence must
use explicit codecs and versioned byte layouts. Rust native struct layout must
not be serialized directly.

### Replica expectation and chain validation

Replica-side validation is based on explicit expectation:

| Field | Meaning |
| --- | --- |
| `tail_lsn` | The highest LSN already durably accepted by the replica, or none at genesis. |
| `expected_next_lsn` | The exact next LSN the replica expects. |

`WalShipmentBatch::validate` must reject:

- source id zero;
- target id zero;
- source not primary;
- target not replica;
- empty batch;
- expectation mismatch;
- first LSN different from expected next LSN;
- first previous-LSN mismatch;
- non-monotonic LSNs;
- numeric LSN gaps;
- previous-LSN chain gaps;
- invalid WAL records.

Successful validation returns an accepted range and the next expected LSN. The
replica can use that result as input to durable append and ACK generation.

### Replica durable ACK

A replica ACK is valid only after the replica has validated and durably accepted
the shipped WAL range.

`WalShippingAck` carries:

| Field | Rule |
| --- | --- |
| `replica_id` | Must identify a required replica when used for retention or quorum evidence. |
| `safe_lsn` | Highest LSN durably received and validated by that replica. |

`WalReplicaSafeLsnTracker` maintains monotonic safe LSNs. A later ACK below a
replica's current safe LSN must not move the safe LSN backward. ACKs from
non-required replicas are rejected when the tracker is being used for required
replica retention.

The retention boundary is the minimum safe LSN across required replicas. A WAL
segment needed by any required replica must not be reclaimed.

### Backpressure

Backpressure is the replica's bounded request to reship from a known point:

| Field | Meaning |
| --- | --- |
| `replica_received_lsn` | Highest LSN the replica has durably accepted. |
| `replica_expected_next_lsn` | Next LSN the replica expected when it detected lag or a gap. |

Backpressure coordinates do not prove the requested bytes are safe. The primary
must still apply the durable source condition, envelope validation, and fencing
policy before reshipping.

### Fencing relationship

WAL shipping feeds quorum and fencing with failure evidence. It does not own the
final quorum decision.

Shipping must emit or preserve evidence for:

| Failure | Required evidence | Quorum/fencing use |
| --- | --- | --- |
| Replica disconnect | replica id, connection identity, last shipped LSN, last ACKed safe LSN | Recompute quorum and decide write admission. |
| Checksum mismatch | segment descriptor, checksum evidence, replica id, LSN range | Fence the connection and block if quorum policy requires it. |
| LSN gap | expected LSN, observed LSN, previous LSN, replica id | Request backpressure or quarantine; block if chain safety cannot be proven. |
| Unknown error | bounded reason, replica id, last known safe LSN | Fail closed in quorum-enforced mode unless policy explicitly allows degraded operation. |

In quorum-enforced mode, a primary must block new write visibility if required
replica ACKs or membership quorum cannot be established. In asynchronous mode,
shipping failure does not block by itself, but degraded evidence must be
auditable and cannot support no-loss failover claims without additional proof.

### Promotion relationship

Promotion depends on WAL shipping evidence. A candidate's safe LSN must cover
the visible commit range the cluster intends to preserve.

Promotion evidence must be tied to:

- former primary durable LSN;
- candidate safe LSN;
- voter safe LSNs;
- shipped and received ranges;
- validated contiguous chain;
- ACKs recorded after durable replica receipt;
- fencing context and proposed epoch.

A candidate below the former primary durable LSN cannot be promoted for a
no-loss failover claim. It may be used only under an explicit restore,
forensic, or data-loss-accepted operator procedure outside normal promotion.

### Observability

Shipping must produce bounded trace or audit evidence for:

- segment selected for shipping;
- segment rejected before send;
- segment accepted by replica;
- replica durable ACK;
- backpressure request;
- checksum mismatch;
- chain gap;
- fencing event;
- retention blocked by required replica safe LSN.

Evidence must include identifiers, LSNs, counters, checksums, policy versions,
and trace ids as applicable. It must not include secret-bearing values or
unbounded raw WAL bodies.

## Validation

Documentation acceptance checks:

- The spec states that only durable primary WAL is shippable.
- The spec states that replica ACKs require durable replica receipt.
- The spec requires contiguous LSN and previous-LSN validation.
- The spec links shipping failures to quorum and fencing evidence.
- The spec blocks retention while required replicas lag.
- The spec does not introduce SQL, gRPC, runtime JSON defaults, native-struct
  persistence, GPU critical-path work, or Procedure bypasses.
- The spec distinguishes runtime-free contract evidence from release-ready
  end-to-end HA/DR.

Current targeted code evidence:

```powershell
cargo test -p andromeda-storage --test wal_shipping_reclaimability_contract
cargo test -p andromeda-storage --test hadr_quorum_contract
```

Release validation must add:

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
cargo test --doc --workspace
```

Before release approval for HA/DR WAL shipping, add crash/recovery or targeted
integration tests that prove:

- a primary never ships records above `primary_durable_lsn`;
- a replica never ACKs records that were not durably accepted;
- gaps and broken previous-LSN chains are rejected before ACK;
- a stale ACK cannot move a replica safe LSN backward;
- a required lagging replica blocks reclaimability;
- a promoted replica's safe LSN survives restart and covers accepted commits;
- shipping evidence survives enough to explain fencing and promotion decisions.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| A shipment contains LSNs above `primary_durable_lsn`. | Shipping used buffered state instead of durable WAL. | Reject the shipment and fix source selection to use durable LSN evidence. |
| Replica ACK advances before fsync or durable append. | ACK was emitted from receive memory instead of durable storage. | Reject the ACK as promotion or retention evidence. |
| `WalShipmentBatch` rejects the first record. | Replica expectation or previous-LSN chain is wrong. | Compare `tail_lsn`, `expected_next_lsn`, first record LSN, and first `previous_lsn`. |
| Numeric LSN gap is accepted by a caller. | Caller bypassed `WalShipmentBatch::validate`. | Treat the range as unsafe and require validation before ACK. |
| Retention deletes a segment needed by a required replica. | Required replica safe-LSN tracking was bypassed or incomplete. | Recompute retention boundary from required replica ACKs and preserve the segment. |
| Quorum promotion ignores WAL shipping ACKs. | Promotion evidence lacks safe-LSN linkage. | Reject no-loss promotion until candidate safe LSN is tied to validated durable shipping evidence. |
| A checksum mismatch is logged but writes continue in quorum mode. | Fencing policy or quorum decision was not applied. | Emit fencing evidence and block visibility when quorum cannot be proven. |
| Raw WAL bytes appear in trace evidence. | Observability payload is unbounded or sensitive. | Replace raw bytes with LSN ranges, checksums, offsets, and bounded diagnostic ids. |

## References

- `documentations/specs/WalRecord_v0.md`
- `documentations/specs/HadrQuorumFencing_v0.md`
- `documentations/specs/RecoveryReport_v0.md`
- `documentations/governance/decisions/DEC-019-f1-wal-shipping-runtime.md`
- `documentations/governance/decisions/DEC-020-quorum-runtime.md`
- `crates/andromeda-storage/src/hadr/shipping_runtime.rs`
- `crates/andromeda-storage/src/hadr/shipping_runtime/segment.rs`
- `crates/andromeda-storage/src/hadr/shipping_runtime/flow_control.rs`
- `crates/andromeda-storage/src/hadr/shipping_runtime/fencing.rs`
- `crates/andromeda-storage/src/write_ahead_log/shipping.rs`
- `crates/andromeda-storage/src/write_ahead_log/shipping/batch.rs`
- `crates/andromeda-storage/src/write_ahead_log/shipping/ack.rs`
- `crates/andromeda-storage/tests/wal_shipping_reclaimability_contract.rs`
- `crates/andromeda-storage/tests/hadr_quorum_contract.rs`
