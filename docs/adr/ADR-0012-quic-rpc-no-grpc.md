# ADR-0012: QUIC/RPC No-gRPC Documentation Acceptance

## Status

Accepted

## Purpose

Record the Lot 5 / WR5-DOC documentation acceptance boundary for QUIC, RPC,
security admission, and audit ledger specifications.

This ADR confirms that Andromeda continues to use a custom typed RPC protocol
over QUIC. It also confirms that documentation and specifications must not imply
gRPC, runtime JSON defaults, ad hoc SQL, a completed durable IAM runtime, an
implemented Admin RPC audit read endpoint, or audit records as database truth.

## Scope

This ADR applies to documentation and specification acceptance for:

- `documentations/specs/FrameHeader_RPC_v0.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `documentations/specs/AuditLedger_v0.md`
- `documentations/governance/decisions/DEC-040-rpc-quic-security-boundary.md`
- `documentations/governance/decisions/DEC-041-security-contract-boundary.md`
- Lot 5 references in the implementation roadmap and worker execution matrix

The accepted documentation boundary is additive. This ADR is documentation
acceptance only; it does not approve or reject Rust, fuzz, CI, or test changes
that may be committed in the broader Lot 5 checkpoint. Code, fuzz, CI, and test
changes require their own validation evidence and must not cite this ADR as
proof that runtime behavior changed safely.

## Non-goals

This ADR does not:

- introduce gRPC, tonic, generated service definitions, HTTP/2 RPC compatibility,
  or runtime JSON defaults;
- introduce application-facing ad hoc SQL, generic command text, dynamic table
  names, or dynamic predicates;
- authorize bypassing typed, cataloged Procedure contracts;
- create or move crates;
- define an executable QUIC listener, stream manager, IAM store, policy store,
  revocation store, or certificate lifecycle workflow;
- claim that durable IAM or policy-management runtime is complete;
- claim that an Admin RPC audit read endpoint is implemented;
- place audit ledger records in the transaction commit path;
- replace database truth, which remains the latest valid cold snapshot plus
  durable WAL from that snapshot.

## Prerequisites

Before using this ADR as acceptance evidence, reviewers must understand these
active decisions and invariants:

- DEC-017 selects QUIC and keeps concrete Quinn/Rustls runtime behavior behind
  the transport runtime boundary.
- DEC-021 keeps Protobuf schemas message-only, service-free, metadata-before-
  payload, and deterministic.
- DEC-033 defines durable audit ledger evidence and checksum-chain validation
  while keeping audit evidence distinct from database truth.
- DEC-040 defines the Lot 5 RPC, QUIC, IAM, audit, and surface-plane boundary.
- DEC-041 defines `andromeda-security-contract` as runtime-free security
  vocabulary and rejects ordinal enum comparisons for security decisions.
- ADR-0011 keeps `andromeda-rpc-protocol` runtime-free and isolates concrete
  transport runtime behavior in `andromeda-quic`.

## Procedure

Use this ADR as the WR5-DOC acceptance checklist.

1. Treat `andromeda-rpc-protocol` as the runtime-free owner of frame contracts,
   stream roles, codecs, and ResultStream sequencing. It must not own Quinn,
   Rustls, sockets, listener lifecycles, Procedure semantics, IAM policy
   authority, WAL, storage, or recovery behavior.
2. Treat `andromeda-quic` as the concrete transport runtime boundary. It may
   translate QUIC streams into typed Andromeda RPC frames, but it must not define
   Procedure semantics, storage truth, commit visibility, or authorization
   policy.
3. Treat `andromeda-security-contract` as runtime-free vocabulary and explicit
   semantic mappings. It must not be described as durable IAM, a mutable
   `PrincipalRegistry`, a policy store, a revocation store, or a certificate
   extraction runtime.
4. Treat `SecurityAdmission v0` as a pre-transaction contract boundary and
   target evidence model. The current runtime-free Rust contract is a stable
   vocabulary subset; full end-to-end admission evidence for every pre-IAM
   rejection remains future implementation work.
5. Treat `AuditLedger v0` as append-only, checksum-chained durable audit record
   evidence with retention compaction caveats. It is forensic and authorization
   evidence, not database truth and not the transaction commit path.
6. Keep Application, Administration, and Cluster or HA/DR surfaces separate.
   Application routing must carry typed, cataloged Procedure invocation traffic
   only.
7. Use current operator wording for CLI audit tooling only: `audit inspect`,
   `audit verify`, and `audit compact`. Do not document an Admin RPC audit read
   endpoint as implemented until code evidence proves that endpoint.

## Validation

For this documentation-only acceptance, validation is textual and structural:

- Confirm that the new specs cross-reference DEC-040 and DEC-041.
- Confirm that no document claims gRPC, runtime JSON defaults, ad hoc SQL,
  durable IAM completion, implemented Admin RPC audit reads, audit as database
  truth, or audit on the transaction commit path.
- Confirm that the frame header spec describes explicit wire bytes and does not
  serialize Rust native struct layout.
- Confirm that the security admission spec fails closed before transaction
  creation.
- Confirm that the audit ledger spec keeps retention compaction and audit
  inspection bounded and does not replace cold snapshot plus durable WAL truth.

Rust builds are not required for this ADR because the accepted artifact is
documentation-only. If a broader Lot 5 checkpoint includes Rust, fuzz, CI, or
test changes, validate those changes with their owning test and fuzz gates and
cite those commands separately.

## Risks

- Future implementation work can drift from the documentation if dependency
  topology and protocol doctrine scans are not kept active.
- Audit evidence can be overstated as durable database truth unless reviewers
  continue to cite DEC-033 and DEC-040.
- Security contract vocabulary can be overstated as mutable IAM runtime unless
  reviewers continue to cite DEC-041.
- The RPC frame header uses network byte order for the current wire contract.
  That is an explicit network-frame contract and must not be generalized to
  persistent storage formats.

## Troubleshooting

Use this section when reviewing or updating Lot 5 documentation.

| Symptom | Corrective action |
| --- | --- |
| A document mentions gRPC or generated service definitions as an accepted runtime surface. | Replace it with custom Andromeda RPC over QUIC and cite DEC-040 and DEC-021. |
| A document treats `andromeda-security-contract` as an IAM store. | Replace it with runtime-free vocabulary and cite DEC-041. |
| A document says audit records are database truth. | Replace it with forensic evidence and cite the latest valid cold snapshot plus durable WAL truth rule. |
| A document says an Admin RPC audit read endpoint is implemented. | Replace it with current CLI audit tooling wording until code evidence proves the endpoint. |
| A document implies a transaction can start before admission. | Require `SecurityAdmission v0` before transaction creation. |

## References

- `AGENTS.md`
- `docs/AGENTS.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `documentations/governance/decisions/DEC-017-quic-runtime-dependency.md`
- `documentations/governance/decisions/DEC-021-protobuf-schema-contract.md`
- `documentations/governance/decisions/DEC-033-durable-audit-ledger.md`
- `documentations/governance/decisions/DEC-040-rpc-quic-security-boundary.md`
- `documentations/governance/decisions/DEC-041-security-contract-boundary.md`
- `documentations/specs/FrameHeader_RPC_v0.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `documentations/specs/AuditLedger_v0.md`
