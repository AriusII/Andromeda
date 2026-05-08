# HADR Quorum and Fencing v0 Specification

## Purpose

Define the accepted v0 contract for Andromeda single-primary HA/DR quorum,
fencing, promotion, epoch, and no-split-brain behavior.

This specification is a safety contract. It does not make runtime memory,
temporary files, benchmark output, GPU output, or audit-only data database
truth. A primary can make writes visible only when the operation is authorized
by the active fencing token and the WAL durability and quorum gates required by
the active replication mode have passed.

## Scope

This specification applies to the HA/DR quorum and fencing control path:

- durable HADR node identities, roles, and role epochs;
- quorum membership snapshots and quorum-size computation;
- single-primary fencing tokens;
- promotion vote evaluation and audit evidence;
- write admission in asynchronous and quorum-enforced replication modes;
- no-split-brain gates for promotion and manifest publication;
- evidence that must exist before a node is treated as primary.

It covers the contract used by the following current Rust surfaces:

- `crates/andromeda-storage/src/hadr/types.rs`
- `crates/andromeda-storage/src/hadr/fencing.rs`
- `crates/andromeda-storage/src/hadr/quorum.rs`
- `crates/andromeda-storage/src/hadr/quorum_runtime.rs`
- `crates/andromeda-storage/src/hadr/cluster_security.rs`
- `crates/andromeda-storage/src/hadr/membership_store.rs`
- `crates/andromeda-storage/src/hadr/membership_persistence_format.rs`
- `crates/andromeda-storage/src/hadr/promotion_boundary.rs`
- `crates/andromeda-storage/src/hadr/promotion_quorum_evidence.rs`

## Current Implementation Status

The repository contains runtime-free quorum, promotion, fencing, membership, and
cluster-security contract code with integration tests. This document is not a
release approval for end-to-end failover. Network transport, cluster manifest
publication, crash/recovery drills, fencing-token propagation to every write
path, and operator runbooks still require release evidence.

Implemented evidence currently includes:

- `HadrEpoch`, `HadrNodeId`, `HadrNodeRole`, and `HadrNodeState` domain types;
- `HadrFencingToken`, `HadrFencingContext`, and `enforce_fencing_token`;
- `HadrQuorumMembership`, `HadrPromotionVote`, `HadrPromotionRequest`, and
  `evaluate_promotion`;
- `HadrAuditRecord` for replayable promotion evidence;
- `QuorumMembership`, `QuorumConsensus`, `ReplicationMode`, `FencingPolicy`,
  `FencingEvent`, and `decide_fencing` in the runtime quorum module;
- `HadrClusterSecurityEvidence` and `HadrClusterManifestUpdateEvidence` for
  cluster-surface permission and quorum-gated manifest update evidence.

## Non-goals

This specification does not:

- introduce ad hoc SQL, dynamic command text, or Procedure-contract bypasses;
- expose HA/DR, promotion, fencing, or manifest update operations through the
  Application surface;
- define a Byzantine quorum or tolerate malicious quorum voters;
- define a dynamic reconfiguration consensus protocol;
- define QUIC stream framing or network retry behavior;
- authorize a primary to accept visible writes without durable WAL;
- serialize Rust native structs directly to disk or network;
- make audit records, metrics, traces, RAM, or temporary state database truth;
- claim that end-to-end failover is release-ready without crash/recovery and
  fencing validation.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `documentations/governance/decisions/DEC-019-f1-wal-shipping-runtime.md`
- `documentations/governance/decisions/DEC-020-quorum-runtime.md`
- `documentations/specs/WalShipping_v0.md`
- `documentations/specs/WalRecord_v0.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `documentations/specs/AuditLedger_v0.md`
- `crates/andromeda-storage/src/hadr/types.rs`
- `crates/andromeda-storage/src/hadr/fencing.rs`
- `crates/andromeda-storage/src/hadr/quorum.rs`
- `crates/andromeda-storage/src/hadr/quorum_runtime.rs`
- `crates/andromeda-storage/src/hadr/cluster_security.rs`
- `crates/andromeda-storage/tests/quorum_membership_contract.rs`
- `crates/andromeda-storage/tests/hadr_quorum_contract.rs`

## Procedure

### Control-plane ownership

HA/DR quorum, fencing, promotion, demotion, and manifest publication are
Cluster or Administration surface operations. They are not Application-surface
features and must not be exposed through typed application Procedures.

The control plane must enforce:

- authenticated node or service identity;
- explicit operation permission;
- current policy-version evidence;
- a durable membership or manifest snapshot;
- a replayable audit record for every allow or reject decision.

### Identity and role model

Each HADR participant has a stable `HadrNodeId`. Node id zero is invalid for
durable quorum membership.

A node role is one of:

| Role | Meaning | Write authority |
| --- | --- | --- |
| `Primary` | The node currently authorized by the active fencing token. | Allowed only under the matching active token and WAL gates. |
| `Replica` | A read-only follower that receives and validates WAL. | Not allowed. |
| `Candidate` | A replica staging promotion for a proposed epoch. | Not allowed until promotion commits and a token is active. |

A node must not silently change role. Role changes require a durable epoch
advance, quorum evidence, security evidence, and audit evidence.

### Epoch model

Andromeda uses epochs to prevent stale primaries and split-brain.

| Epoch field | Owner | Meaning |
| --- | --- | --- |
| `HadrEpoch` | Durable HADR control plane | Role and fencing epoch. Successful promotion strictly advances it. |
| `QuorumMembership::epoch()` | Quorum runtime membership | Runtime membership-health topology epoch. It changes when membership health or membership shape changes. |
| `HadrClusterManifestVersion` | Cluster manifest publication | Durable manifest version that records accepted cluster-state changes. |

Rules:

1. A promoted primary must own an active `HadrFencingToken` that binds
   `(primary_id, epoch)`.
2. A proposed promotion epoch must be strictly greater than every accepted
   source of epoch evidence: active token epoch, highest observed fencing
   context epoch, voter observed epochs, and candidate observed epoch.
3. A node that observes a higher epoch than its own active token must stop
   acting as primary before accepting more visible work.
4. Epoch zero is bootstrap evidence only. Release configurations must not use
   epoch zero as an accepted production-primary fencing token.
5. Epoch advancement is durable cluster state. Runtime memory can cache it, but
   runtime memory is not the source of truth.
6. Epoch overflow is a fail-closed condition.

Current code enforces active-token and voter-observed epoch ordering in
`evaluate_promotion`. Release wiring must also prove that the candidate's
observed epoch and the durable manifest epoch are included in the same
monotonic evidence chain before promotion is visible.

### Quorum membership

Quorum membership is a durable set of eligible participants. Quorum size is
`floor(member_count / 2) + 1`.

Membership rules:

- membership must be non-empty;
- duplicate node ids are invalid;
- every voter in a promotion attempt must be a member;
- duplicate voters are invalid;
- non-member votes are invalid;
- membership publication requires cluster-surface permission evidence;
- membership and manifest updates require quorum-granted evidence;
- removing or adding a voting member must advance durable membership evidence.

Runtime health states can be `Alive`, `Suspect`, or `Dead`. Health state is an
input to write admission and promotion ranking, but it is not sufficient to
publish cluster truth without durable membership and manifest evidence.

### Write admission

Write admission is separate from promotion. A node can admit visible writes only
when all required gates pass:

1. the node presents the active fencing token;
2. the token matches the active primary id and epoch;
3. WAL records for the transaction are durable before visibility;
4. the active replication mode's acknowledgement policy is satisfied;
5. the current fencing decision allows writes;
6. security and surface gates have already admitted the request.

Replication modes:

| Mode | Minimum replica ACKs before visibility | Quorum-loss behavior |
| --- | ---: | --- |
| `Asynchronous` | `0` | Quorum loss does not block by itself, but degraded evidence must be emitted. |
| `QuorumEnforced` | Active majority quorum | Quorum loss blocks new write visibility. |

Asynchronous mode can improve availability, but it cannot support a no-data-loss
claim after primary failure unless separate durable evidence proves the
replica's safe LSN covers every visible commit.

### Fencing

Fencing rejects work from a stale, mismatched, or unrecorded primary token.

The active token is `(primary_id, epoch)`. Every primary-side operation that can
lead to visible mutation must carry a token. `enforce_fencing_token` accepts a
presented token only when it matches the active token exactly. It rejects:

- no active token;
- presented epoch lower than the active epoch;
- presented primary id different from the active primary at the same epoch;
- presented epoch higher than the active epoch, because the cluster has not
  recorded that promotion.

Fencing events include:

| Event | Meaning | Required control-plane response |
| --- | --- | --- |
| `ReplicaDisconnected` | A replica connection dropped or missed required heartbeats. | Recompute quorum state and decide allow or block. |
| `ReplicaChecksumMismatch` | A replica rejected received bytes or reported checksum drift. | Fence the replica connection, preserve evidence, and decide write admission. |
| `ReplicaLsnGap` | A replica observed a non-contiguous WAL chain. | Preserve gap evidence, request backpressure when safe, and decide fencing. |
| `Unknown` | The failure cannot be classified. | Fail closed for quorum-enforced writes unless policy explicitly allows degraded operation. |

`FencingPolicy::Allow` is an explicit degraded policy. It must be observable,
audited, and disableable. It is not a mission-critical consistency default.

### Promotion

Promotion converts a replica or candidate into the single primary for a new
epoch. Promotion is a pure decision followed by durable publication.

A promotion request must include:

- candidate id, role, observed epoch, safe LSN, and divergence evidence;
- proposed epoch;
- voter ids, voter observed epochs, voter safe LSNs, divergence evidence, and
  granted or denied status;
- the durable quorum membership snapshot;
- the current fencing context;
- security evidence for the cluster promotion operation.

Promotion is approved only when all gates pass:

1. the candidate role is `Replica` or `Candidate`;
2. the candidate is a quorum member;
3. every vote is from a unique quorum member;
4. the proposed epoch is monotonic;
5. no active token has an epoch greater than or equal to the proposed epoch;
6. granted votes meet quorum size;
7. candidate safe LSN is at least the maximum granting-voter safe LSN;
8. no candidate or voter divergence evidence exists at or below the candidate
   safe LSN;
9. security admission and audit evidence authorize promotion;
10. the resulting token and manifest update are durable before the new primary
    accepts visible writes.

The candidate safe LSN must cover every visible commit that the cluster intends
to preserve. The current pure promotion engine compares candidate safe LSN to
granting voter safe LSNs. Release wiring must also tie voter safe LSNs to WAL
shipping evidence and the former primary durable LSN.

### No split-brain

Split-brain means two nodes can both accept primary work for overlapping cluster
truth. The v0 contract prevents it through combined gates:

- strict single active fencing token;
- monotonic epochs;
- majority quorum for promotion;
- durable manifest publication before new-primary visibility;
- stale-token rejection on the old primary;
- WAL safe-LSN proof for the promoted candidate;
- audit evidence for every promotion and rejection.

If the cluster cannot prove these gates, it must block promotion or block write
visibility. Operator convenience must not downgrade the no-split-brain gate.

### Manifest publication

Promotion approval is not complete until the cluster manifest records the new
primary and epoch. Manifest update evidence must include:

- previous and proposed epoch;
- previous and proposed manifest version;
- quorum size;
- granted vote count;
- cluster operation;
- security audit evidence;
- human-readable reason bounded to an operator-safe string.

The manifest update must fail closed when:

- proposed epoch does not advance;
- proposed manifest version does not advance;
- quorum size is zero;
- granted votes are below quorum size;
- the operation was authorized on a non-cluster surface;
- the permission does not match the requested cluster operation;
- the security decision was not allowed.

### Evidence gates

The following evidence is required before accepting a primary as authoritative:

| Gate | Required evidence |
| --- | --- |
| Identity | `HadrNodeId`, authenticated cluster identity, and security audit trace. |
| Membership | Durable quorum membership snapshot and quorum size. |
| Epoch | Highest observed epoch, active token, proposed epoch, voter epochs, and manifest epoch. |
| Fencing | Active `HadrFencingToken` and enforcement result for primary-side work. |
| WAL durability | Former primary durable LSN and replica safe LSN evidence. |
| WAL shipping | Validated contiguous shipment ranges and replica ACKs. |
| Promotion | `HadrPromotionRequest`, votes, outcome, and `HadrAuditRecord`. |
| Manifest | `HadrClusterManifestUpdateEvidence` with quorum-granted update evidence. |
| Recovery | Crash/recovery evidence proving no visible commit appears without durable WAL. |
| Audit | Durable audit record linking actor, permission, operation, outcome, and policy version. |

Evidence must be bounded, typed, replayable, and safe to retain. Raw WAL bodies,
secret-bearing values, private keys, credentials, and unbounded payload excerpts
must not be embedded in quorum or promotion evidence.

## Validation

Documentation acceptance checks:

- The spec preserves the single-primary invariant.
- The spec requires durable WAL before visible commit.
- The spec treats fencing tokens as required primary authority.
- The spec rejects stale epochs, duplicate voters, non-member voters, and
  active-token conflicts.
- The spec states that HA/DR operations are not Application-surface features.
- The spec separates runtime membership health from durable cluster truth.
- The spec requires promotion, manifest, WAL, security, and audit evidence.
- The spec does not introduce SQL, gRPC, runtime JSON defaults, native-struct
  persistence, GPU critical-path work, or Procedure bypasses.

Current targeted code evidence:

```powershell
cargo test -p andromeda-storage --test quorum_membership_contract
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

Before release approval for C4/C5 HA/DR behavior, add crash/recovery tests that
prove:

- a stale primary token cannot publish a visible write after promotion;
- a promoted primary cannot accept visible writes until its token and manifest
  are durable;
- a candidate below the former primary durable LSN cannot be promoted for
  no-loss failover;
- quorum loss in quorum-enforced mode blocks new visibility;
- restart after failover preserves the active epoch and fencing token.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Two nodes claim primary at the same epoch. | Fencing token enforcement or manifest publication was bypassed. | Block writes, preserve evidence, and require operator recovery before accepting either node. |
| Promotion succeeds with duplicate voters. | Vote roster validation is missing or was bypassed. | Reject promotion and require a unique quorum-member roster. |
| Promotion succeeds while an active token has the same or higher epoch. | Split-brain gate was bypassed. | Reject promotion and record `SplitBrainActiveToken` evidence. |
| Old primary continues writing after promotion. | Old token was not fenced at write admission. | Enforce `enforce_fencing_token` before every primary-side visible mutation. |
| Quorum loss does not block writes in quorum mode. | Replication mode or fencing policy is misconfigured. | Verify `ReplicationMode::QuorumEnforced`, `FencingPolicy::BlockOnQuorumLoss`, and membership health evidence. |
| Candidate safe LSN is below a granting voter. | Candidate has not caught up or voter evidence is stale. | Reject promotion and resume WAL shipping or restore from a safe source. |
| A promotion audit record lacks the active token or highest observed epoch. | Evidence capture is incomplete. | Reject the record as release evidence and add the missing fencing context. |
| HA/DR operation appears on the Application surface. | Surface isolation was violated. | Move the operation to Cluster or Administration surface and require security evidence. |

## References

- `documentations/governance/decisions/DEC-019-f1-wal-shipping-runtime.md`
- `documentations/governance/decisions/DEC-020-quorum-runtime.md`
- `documentations/specs/WalShipping_v0.md`
- `documentations/specs/WalRecord_v0.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `documentations/specs/AuditLedger_v0.md`
- `documentations/specs/RecoveryReport_v0.md`
- `crates/andromeda-storage/src/hadr/types.rs`
- `crates/andromeda-storage/src/hadr/fencing.rs`
- `crates/andromeda-storage/src/hadr/quorum.rs`
- `crates/andromeda-storage/src/hadr/quorum_runtime.rs`
- `crates/andromeda-storage/src/hadr/cluster_security.rs`
- `crates/andromeda-storage/src/hadr/membership_store.rs`
- `crates/andromeda-storage/src/hadr/membership_persistence_format.rs`
- `crates/andromeda-storage/src/hadr/promotion_boundary.rs`
- `crates/andromeda-storage/src/hadr/promotion_quorum_evidence.rs`
- `crates/andromeda-storage/tests/quorum_membership_contract.rs`
- `crates/andromeda-storage/tests/hadr_quorum_contract.rs`
