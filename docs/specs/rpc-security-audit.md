# RPC, Security, And Audit

## Purpose

This spec defines the runtime-free RPC frame contract, ResultStream ordering,
surface separation, security admission, DecisionTrace, and AuditLedger rules.
It preserves the custom Andromeda RPC-over-QUIC boundary and the fail-closed
rule that denied admission creates no transaction.

## RPC frame header

The runtime-free protocol owner controls frame and stream contracts. The QUIC
transport owner maps those contracts onto concrete transport behavior.

| Field | Rule |
| --- | --- |
| Header length | Must equal 52 bytes. |
| Codec version | Must equal `1`. |
| Frame type | Must be a locked frame type code. |
| Request and session id | Unsigned correlation identifiers. |
| Transaction id | Present only when `tx_id_present` is `1`; otherwise zero. |
| Payload length | Must be at most 16 MiB. |
| Flags | Reserved in v0 and must be zero. |
| Header CRC | CRC-32 over the fixed header with the CRC field zeroed. |

Accepted frame codes are `HELLO`, `AUTH`, `CONTRACT_REQUEST`,
`CONTRACT_RESPONSE`, `RPC_EXECUTE_REQUEST`, `RPC_METADATA`, `RPC_BATCH`,
`RPC_COMPLETION`, `ERROR`, and `TELEMETRY_SOFT_SIGNAL`. Unknown frame codes,
unknown stream roles, nonzero reserved fields, CRC mismatches, and payload
length mismatches are protocol errors.

Telemetry datagrams are for soft signals only. Contract-bound RPC payloads must
use reliable stream roles, not datagrams.

## ResultStream ordering

Result metadata must precede result payload batches unless a Procedure contract
explicitly allows streaming-unknown metadata. Completion must be terminal.
Result batches before metadata, payload after completion, mismatched stream
ids, and row-count metadata drift are protocol errors.

## Surface separation

| Surface | Allowed purpose | Application routing rule |
| --- | --- | --- |
| Application | Business Procedure invocation and allowed contract metadata reads. | May execute typed cataloged Procedures only. |
| Administration | DefinitionBatch import, catalog administration, Procedure Store, backup and restore control, debug, certificates, IAM, policies, and maintenance. | Must not be tunneled through Application. |
| Monitoring | Bounded health, metrics, and status reads. | Must remain read-only and policy-scoped. |
| BackupAgent | Backup, backup validation, and retention work. | Must not be tunneled through Application. |
| Cluster or HA/DR | WAL shipping, quorum, fencing, manifests, health, membership, promotion, and replica coordination. | Must not be tunneled through Application. |
| Forensic | Forensic startup, hold, read-only inspection, corruption-boundary investigation, and incident evidence preservation. | Must route through the owning administrative or recovery control path. |

Surface mismatch rejects before Procedure dispatch and before transaction
creation.

## Security admission order

Security admission is fail-closed and must run in this order:

1. Resolve typed certificate, session, principal, and principal status evidence.
2. Classify listener surface and requested operation family.
3. Look up typed Procedure contract or operation descriptor evidence.
4. Evaluate required permission families through explicit mappings.
5. Evaluate policy, resource, and IO budgets.
6. Emit bounded admission decision and required audit evidence.
7. Pass only accepted admission evidence to execution or privileged control.

Permission evaluation must not run against partial identity. Required
permissions are read from the Procedure contract or typed operation descriptor,
not from client-provided text. Explicit deny wins over allow. Disabled,
expired, revoked, unknown, or mismatched principals fail closed.

The security contract crate is runtime-free vocabulary. It is not an IAM
runtime, policy store, revocation store, audit ledger, or TLS runtime owner.

## Admission result

| Decision | Required effect |
| --- | --- |
| `Accept` | Return a bounded admission token or evidence record for the next layer. |
| `DenyIdentity` | Reject before surface dispatch with `NoTransaction`. |
| `DenySurface` | Reject before contract or operation dispatch with `NoTransaction`. |
| `DenyContract` | Reject before Procedure or privileged operation dispatch with `NoTransaction`. |
| `DenyPermission` | Reject before Procedure or privileged operation dispatch with `NoTransaction`. |
| `DenyPolicy` | Reject before Procedure or privileged operation dispatch with `NoTransaction`. |
| `DenyResource` | Reject before Procedure or privileged operation dispatch with `NoTransaction`. |
| `DenyAuditUnavailable` | Reject when required durable audit evidence cannot be produced before decision visibility. |

Accepted or denied IAM decisions must include trace id, request id, session id,
principal or certificate evidence, surface, contract or operation descriptor,
permission family, policy version, reason code, `NoTransaction` marker for
rejections, and durable audit evidence when required by the owning path.

## DecisionTrace

DecisionTrace records decision evidence. It is not execution and not database
truth.

| Field class | Bound |
| --- | --- |
| Reason text | Maximum 512 UTF-8 bytes after sanitization. |
| Diagnostic excerpt | Maximum 256 sanitized UTF-8 bytes and never a payload body. |
| Evidence references | Maximum 32 references per trace. |
| Evidence label | Maximum 64 UTF-8 bytes. |
| Evidence digest | Fixed-size digest or typed hash only. |
| Metrics | Fixed numeric fields with units. |

Decision families include Procedure contract, security admission, protocol
admission, optimizer or plan, resource or IO, GPU policy, transaction or WAL,
catalog or manifest, and recovery or forensic decisions. Commit-visible,
rollback-durable, WAL-flush, manifest-switch, recovery-startup, and catalog
publication decisions require matching durable evidence.

DecisionTrace replay may rebuild indexes or reports. It must not re-execute
Procedures or change database state.

## AuditLedger

AuditLedger records durable forensic and authorization evidence. It is not
database truth and not a replacement for WAL-backed storage recovery.

| Evidence | Required content |
| --- | --- |
| Record identity | Event id, trace id, durable audit family, sequence number. |
| Principal binding | Principal id when known, certificate fingerprint when applicable, surface, permission, request id, session id. |
| WAL evidence | Record LSN, durable LSN, payload checksum when the sink is WAL-backed. |
| Chain evidence | Previous and current chain checksums for journal continuity. |
| Retention evidence | Retention boundary, policy hold, compaction eligibility. |
| Replay behavior | Forensic-only replay, decision-index rebuild, or corruption-boundary behavior. |
| Payload summary | Sanitized event kind and bounded decision metadata. |

Audit event families include security, administration, catalog, admission,
HA/DR, backup, restore, forensic, recovery, and generic forensic events.
Compaction must preserve retained checksum and chain evidence. Audit records
must not include secret-bearing fields.

## Validation gates

- Malformed frame tests must reject before payload interpretation.
- ResultStream tests must reject payload before metadata and payload after
  completion.
- Security tests must prove no transaction is created for denial.
- Surface tests must reject Administration, HA/DR, BackupAgent, and Forensic
  operations on Application.
- Audit tests must prove bounded sanitized payloads, chain validation, and
  forensic-only replay.
