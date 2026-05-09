# ADR-0014: Binary Format and Endian Policy

## Status

Accepted

## Purpose

Define Andromeda's canonical policy for persistent and network binary bytes.

Persistent and network formats must use explicit codecs. They must not serialize
Rust native struct layout to disk or network. New or revised Andromeda-owned
fixed-width binary fields use little-endian encoding unless a specific versioned
format decision records a narrower exception.

## Decision

Andromeda persistent formats use explicit little-endian codecs as the canonical
binary representation. This applies to WAL records, page headers and trailers,
segment metadata, manifests, cold snapshots, catalog WAL records, durable audit
records, storage fingerprints, and other durable bytes.

Andromeda-owned network binary formats also use explicit codecs and should use
little-endian fixed-width fields for new or revised custom frame bytes. Existing
versioned network contracts that document a different byte order remain explicit
compatibility exceptions until a separate protocol migration changes them.

The current RPC frame header v0 network-byte-order contract is one such
exception. It is not a storage-format precedent and does not allow Rust native
struct layout serialization. Future exceptions require an ADR or versioned spec,
golden byte vectors, compatibility tests, and fail-closed handling for
unsupported versions.

Standardized external encodings, such as Protobuf payload encoding, follow their
own explicit specification. They still require schema governance and must not be
treated as Rust native struct layout.

## Scope

This ADR applies to:

- WAL records and WAL file headers;
- database pages, page trailers, extents, segments, and cold snapshots;
- manifests, root pointers, checkpoints, and recovery metadata;
- catalog WAL records, catalog object fingerprints, contract hashes, and
  deterministic hash sinks;
- custom RPC frame bytes, ResultStream metadata frames, and other
  Andromeda-owned protocol bytes;
- Protobuf payload contracts where Andromeda owns schema compatibility and
  descriptor governance;
- golden vectors, format fingerprints, migration readers, recovery readers, and
  protocol compatibility tests.

## Non-goals

This ADR does not:

- define the complete layout of any individual WAL, page, segment, manifest,
  catalog, RPC, or Protobuf format;
- change the existing RPC frame header v0 wire bytes;
- introduce gRPC, runtime JSON defaults, application-facing ad hoc SQL, dynamic
  table names, dynamic predicates, or shape-shifting Procedure returns;
- authorize code changes, data-file migration, or protocol migration by itself;
- allow audit records, benchmark output, GPU output, or temporary files to
  become database truth;
- make `#[repr(C)]`, `#[repr(packed)]`, pointer casts, `transmute`, or native
  Rust struct layout acceptable as a disk or network serialization contract.

## Prerequisites

Before promoting a binary format beyond prototype status, reviewers must know:

- the owning crate and subsystem;
- whether the bytes are durable storage, network protocol, diagnostic output,
  temporary scratch data, or rebuildable cache data;
- the format identity, version field, feature flags, and compatibility policy;
- the length bounds and malformed-input rejection behavior;
- the checksum, digest, chain hash, or descriptor hash coverage;
- the recovery or protocol behavior when a reader sees an unknown version,
  unsupported feature, truncated payload, or checksum mismatch.

## Procedure

Use this procedure for new or revised persistent and network binary formats.

1. Define the format identity.
   - Include a magic value, format name, version, and owner.
   - State whether the format is durable truth, forensic evidence, network
     protocol, diagnostic output, temporary data, or rebuildable cache data.
2. Encode explicitly.
   - Write and read every field through codec functions.
   - Do not serialize Rust structs directly.
   - Do not rely on field order, padding, alignment, enum discriminants, or
     platform layout.
3. Use canonical endian rules.
   - Encode new or revised Andromeda-owned fixed-width integer fields as
     little-endian.
   - Document any network compatibility exception in a versioned spec or ADR.
   - Keep exceptions local to the named format and version.
4. Bound all variable data.
   - Length-prefix variable payloads with explicit maximum sizes.
   - Reject truncated, overlong, duplicate, or trailing bytes unless the format
     explicitly reserves an extension area.
   - Validate counts before allocation.
5. Cover integrity and compatibility.
   - Include checksum, digest, chain hash, descriptor hash, or equivalent
     integrity coverage appropriate to the format.
   - Cover the fields needed to detect wrong format, wrong version, wrong
     endian handling, truncation, and unsupported feature bits.
6. Reject before mutation.
   - Recovery readers must reject unknown durable formats before redo mutates
     state.
   - Protocol readers must reject unknown or unsupported frame versions before
     dispatch.
   - Migration readers must use explicit dual-read, rewrite, rebuild, or
     fail-closed behavior. They must not reinterpret legacy bytes as the new
     layout.
7. Preserve golden evidence.
   - Add golden byte vectors for every durable or protocol format promoted
     beyond prototype status.
   - Include malformed-input vectors for wrong endian, wrong version, truncated
     payloads, unknown flags, and checksum mismatch.
   - Add cross-architecture validation or deterministic byte-level tests where
     endian behavior matters.

## Validation

For this documentation-only ADR, validation is textual and structural:

- Confirm that the ADR requires explicit codecs for persistent and network
  bytes.
- Confirm that the ADR rejects Rust native struct layout for disk and network.
- Confirm that the ADR sets little-endian as the canonical default for new or
  revised Andromeda-owned fixed-width binary fields.
- Confirm that the ADR records the current RPC frame header v0 byte-order
  exception as a named compatibility exception, not as a storage precedent.

For future implementations, reviewers should require the relevant gates:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
```

Persistent formats also require round-trip tests, golden vectors, malformed
input tests, unknown-version rejection, and recovery validation before redo.

Network formats also require protocol compatibility tests, malformed-frame
tests, descriptor or discriminator stability checks, and fail-closed dispatch
behavior.

Parser and codec changes should add property tests or fuzz targets for
truncated, overlong, wrong-endian, wrong-version, and random input.

## Risks

- Existing network-byte-order compatibility exceptions can drift into new custom
  protocol formats unless each exception is named and versioned.
- Developers can mistake `#[repr(C)]` or `#[repr(packed)]` for a stable storage
  or wire contract.
- Golden vectors can become stale if generated from the implementation instead
  of reviewed as normative bytes.
- Recovery can become unsafe if unknown durable bytes are partially replayed
  before format validation completes.
- Protobuf governance can be confused with native Rust layout unless schema and
  descriptor rules remain explicit.

## Troubleshooting

Use this table when reviewing binary format changes.

| Symptom | Corrective action |
| --- | --- |
| A document says a Rust struct is the disk or wire layout. | Replace it with an explicit byte layout and codec contract. |
| A new custom binary format omits endian rules. | Require little-endian fixed-width fields or a versioned exception with compatibility evidence. |
| A reader accepts unknown versions or feature bits. | Change the reader to fail closed before mutation or dispatch. |
| A durable format has no magic, version, checksum, or length bounds. | Block promotion beyond prototype status until identity, integrity, and bounds are specified. |
| A migration plan reinterprets legacy bytes as a new layout. | Replace it with dual-read, rewrite, rebuild, or explicit rejection behavior. |
| Golden vectors are generated only from current code. | Add reviewed normative vectors and malformed-input vectors independent of implementation internals. |
| The RPC frame header v0 exception is cited for storage bytes. | Reject the citation and restate that the exception is network-version-specific. |

## References

- `AGENTS.md`
- `docs/AGENTS.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `docs/adr/ADR-0012-quic-rpc-no-grpc.md`
- `docs/adr/OPEN_DECISIONS.md`
- `docs/adr/README.md`
- `docs/adr/README.md`
- `docs/adr/README.md`
- `docs/specs/rpc-security-audit.md`
