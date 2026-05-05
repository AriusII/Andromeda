# CLI — HA/DR Command Reference

**Version:** 1.0.0  
**Updated:** Q1 2026  
**Language:** American English  
**Style:** Microsoft Documentation  

---

## Overview

This document provides a comprehensive reference for the `andromeda-cli` high-availability/disaster recovery (HA/DR) command surface. The CLI enables operators to inspect cluster state, manage node membership, perform promotion and failover operations, and configure quorum fencing policies without direct database access.

All HA/DR commands are dry-run scaffolds until the durable membership store, authorization hook, and audit sink are wired by the HA/DR runtime. Commands marked **[V1+]** require full runtime support.

---

## Command Structure

### Syntax

```text
andromeda-cli hadr <subcommand> [options]
```

### Common Options

| Option | Type | Description |
|--------|------|-------------|
| `--json` | flag | Output results in JSON format (diagnostic only, not default wire format). |
| `--dry-run` | flag | Execute contract validation without mutation or durable side effects. |
| `-h`, `--help` | flag | Display subcommand help text. |

---

## Subcommands

### `hadr status` — Display Cluster State

**Purpose:** Inspect current cluster role, quorum membership, Log Sequence Number (LSN) positions, and replica lag metrics.

**Syntax:**

```bash
andromeda-cli hadr status [--json]
```

**Outputs:**

```json
{
  "cluster_role": "Primary" | "Replica",
  "epoch": <u64>,
  "current_primary": <node_id> | null,
  "replicas": [
    {
      "replica_id": <u64>,
      "health_state": "Alive" | "Behind" | "Failed",
      "received_lsn": <u64>,
      "shipped_lsn": <u64>,
      "lag_bytes": <i64>
    }
  ],
  "durable_lsn": <u64>,
  "committed_lsn": <u64>
}
```

**Field Definitions:**

| Field | Type | Meaning |
|-------|------|---------|
| `cluster_role` | String | Current node role in the cluster. |
| `epoch` | u64 | Membership epoch (incremented on each promotion). |
| `current_primary` | u64 \| null | Node ID of the authoritative primary, or null if unknown. |
| `replicas[]` | Array | Status of each known replica. |
| `replica_id` | u64 | Unique replica node identifier. |
| `health_state` | String | "Alive" (connected and catching up), "Behind" (connected but lagging), or "Failed" (connection lost). |
| `received_lsn` | u64 | Highest LSN durably persisted at this replica. |
| `shipped_lsn` | u64 | Highest LSN sent from the primary to this replica. |
| `lag_bytes` | i64 | Bytes difference between durable_lsn and received_lsn; negative indicates ahead. |
| `durable_lsn` | u64 | Highest LSN durably flushed at the primary. |
| `committed_lsn` | u64 | Highest LSN visible to Procedures (committed state). |

**Example:**

```bash
$ andromeda-cli hadr status
Cluster Role: Primary
Epoch: 42
Current Primary: 1
Replicas:
  ID 2 (Alive):    received_lsn=1048576, lag=0 bytes
  ID 3 (Behind):   received_lsn=1047552, lag=1024 bytes

Durable LSN: 1048576
Committed LSN: 1048576
```

**Error Codes:**

| Code | Meaning |
|------|---------|
| `ClusterUninitialized` | No cluster membership configured. Initialize quorum first. |
| `PrimaryUnreachable` | Primary node is not responding; cluster is read-only until reconnection or failover. |
| `LsnDivergence` | Detected LSN divergence at a replica; manual intervention required. |

---

### `hadr node` — Node Management Contract

**Purpose:** Register, deregister, list, or inspect node membership and configuration.

**Syntax:**

```bash
andromeda-cli hadr node <action> [options]
```

#### `hadr node register <node-id>`

**Purpose:** Register a replica node into the cluster membership.

**Syntax:**

```bash
andromeda-cli hadr node register <node-id> --role replica [--dry-run] [--json]
```

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `<node-id>` | u64 | Yes | Unique node identifier. Must not be the current primary. |
| `--role` | String | No | Node role. Currently only "replica" is accepted. Default: "replica". |
| `--dry-run` | flag | No | Validate contract without mutation. |
| `--json` | flag | No | Output in JSON format. |

**Output:**

```json
{
  "action": "register",
  "node_id": <u64>,
  "role": "replica",
  "dry_run": true | false,
  "would_apply": true | false,
  "quorum_check": <status>,
  "fencing_check": <status>,
  "audit_event": "hadr.node.register.requested",
  "required_permissions": ["UpdateClusterManifest"],
  "failure_mode": null | <error>,
  "message": <string>
}
```

**Constraints:**

- **[V1+]** Requires durable membership store, authorization hook, and audit sink.
- Currently dry-run only; rerun with `--dry-run` to see contract.
- Does not auto-join the node into quorum; explicit promotion or candidate election is required.

**Example:**

```bash
$ andromeda-cli hadr node register 4 --dry-run --json
{
  "action": "register",
  "node_id": 4,
  "role": "replica",
  "dry_run": true,
  "would_apply": false,
  "quorum_check": "passes_static_membership_validation",
  "fencing_check": "no_fencing_token_issued",
  "audit_event": "hadr.node.register.requested",
  "message": "dry-run accepted: replica node registration would require durable membership update and audit emission"
}
```

#### `hadr node deregister <node-id>`

**Purpose:** Remove a replica node from cluster membership (durable deprovisioning).

**Syntax:**

```bash
andromeda-cli hadr node deregister <node-id> --fencing-evidence <evidence-id> [--dry-run] [--json]
```

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `<node-id>` | u64 | Yes | Node to remove. |
| `--fencing-evidence` | String | Yes | Operator-provided fencing evidence ID (e.g., ticket number). |
| `--dry-run` | flag | No | Validate contract without mutation. |
| `--json` | flag | No | Output in JSON format. |

**Constraints:**

- **[V1+]** Requires operator-provided fencing evidence.
- Deregistration preserves quorum (minimum quorum_size members must remain).
- If the node to deregister is a quorum member, a new non-member must be registered first.

**Example:**

```bash
$ andromeda-cli hadr node deregister 4 --fencing-evidence JIRA-12345 --dry-run
Deregistration dry-run for node 4
Status: PASS (quorum preserved)
Fencing Evidence: JIRA-12345
Message: dry-run accepted: node deregistration would require quorum preservation, fencing evidence, durable membership update, and audit emission
```

#### `hadr node list`

**Purpose:** Display all registered cluster nodes and their roles.

**Syntax:**

```bash
andromeda-cli hadr node list [--json]
```

**Output:**

```json
{
  "nodes": [
    { "node_id": <u64>, "role": "Primary" | "Replica" | "Candidate", "state": <string> }
  ],
  "total_members": <u64>,
  "quorum_size": <u64>
}
```

**Example:**

```bash
$ andromeda-cli hadr node list
Registered Nodes (3 total, quorum size: 2):
  Node 1 [Primary]  (Active)
  Node 2 [Replica]  (Alive, LSN lag 0)
  Node 3 [Replica]  (Behind, LSN lag 1024 bytes)
```

#### `hadr node status <node-id>`

**Purpose:** Inspect the detailed status of a specific node.

**Syntax:**

```bash
andromeda-cli hadr node status <node-id> [--json]
```

**Output:** Same structure as `hadr status` but filtered to a single node.

---

### `hadr promote` — Promotion Protocol

**Purpose:** Promote a replica to primary at a specified LSN or at current durable state. Triggers quorum-based election voting.

**Syntax:**

```bash
andromeda-cli hadr promote <replica-id> [--target-lsn <lsn>] [--json] [--dry-run]
```

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `<replica-id>` | u64 | Yes | Replica node ID to promote. |
| `--target-lsn` | u64 | No | Target LSN for promotion. Default: current durable_lsn at primary. |
| `--dry-run` | flag | No | Validate eligibility without execution. |
| `--json` | flag | No | Output in JSON format. |

**Output:**

```json
{
  "success": true | false,
  "new_epoch": <u64>,
  "promoted_replica_id": <u64>,
  "message": <string>
}
```

**Eligibility Criteria (F4 Boundary):**

Promotion is eligible only if all of the following hold (see DEC-024):

1. Replica is a quorum member.
2. Replica `safe_lsn >= primary.durable_lsn` (no data loss).
3. Replica is reachable (has working connection).
4. No active fencing token epoch at or above proposed epoch.
5. Proposed epoch > highest observed epoch (monotonicity).

**Example — Successful Promotion:**

```bash
$ andromeda-cli hadr promote 2 --json
{
  "success": true,
  "new_epoch": 43,
  "promoted_replica_id": 2,
  "message": "Replica 2 promoted to primary (epoch 43)"
}
```

**Example — Eligibility Check Fails:**

```bash
$ andromeda-cli hadr promote 3 --dry-run
Promotion eligibility check for replica 3:
Status: FAILED
Reason: LSN lag (1024 bytes behind durable_lsn)
Message: Replica is not eligible for promotion. Apply log shipping or wait for catch-up.
Error Code: PromotionLsnIneligible
```

**Error Codes:**

| Code | Meaning |
|------|---------|
| `PromotionLsnIneligible` | Replica LSN is behind durable_lsn. |
| `PromotionNotQuorumMember` | Replica is not a quorum member. |
| `PromotionUnreachable` | Replica is not reachable. |
| `PromotionFencingConflict` | Active fencing token blocks promotion. |
| `PromotionEpochRegression` | Proposed epoch is lower than an existing epoch. |

---

### `hadr demote` — Planned Failover

**Purpose:** Demote the current primary to a replica and optionally promote a candidate. Used for planned maintenance or graceful cluster rebalancing.

**Syntax:**

```bash
andromeda-cli hadr demote [--promote-candidate <node-id>] [--json] [--dry-run]
```

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `--promote-candidate` | u64 | No | Node ID to promote to primary after demotion. |
| `--dry-run` | flag | No | Validate plan without execution. |
| `--json` | flag | No | Output in JSON format. |

**Output:**

```json
{
  "success": true | false,
  "new_primary_id": <u64> | null,
  "message": <string>
}
```

**Constraints:**

- Current primary must be running.
- Candidate (if specified) must be eligible for promotion (see `hadr promote`).
- If no candidate is specified, quorum must elect one automatically.

**Example:**

```bash
$ andromeda-cli hadr demote --promote-candidate 2 --dry-run
Planned failover dry-run:
  Current Primary: 1
  Target Candidate: 2
  Status: ELIGIBLE
  Message: dry-run accepted; replica 2 eligible for promotion
```

---

### `hadr quorum` — Quorum Management

**Purpose:** Inspect and configure quorum membership, fencing policies, and membership validation.

**Syntax:**

```bash
andromeda-cli hadr quorum <action> [options]
```

#### `hadr quorum status`

**Purpose:** Display quorum configuration and current membership state.

**Syntax:**

```bash
andromeda-cli hadr quorum status [--json]
```

**Output:**

```json
{
  "total_members": <u64>,
  "quorum_size": <u64>,
  "member_ids": [<u64>, ...],
  "fencing_policy": "Epoch" | "Lease" | "Split-Brain-Detector",
  "fencing_status": "Active" | "Degraded" | "Disabled"
}
```

**Example:**

```bash
$ andromeda-cli hadr quorum status
Quorum Membership:
  Total Members: 3
  Quorum Size: 2
  Member IDs: [1, 2, 3]
  Fencing Policy: Epoch (DEC-020)
  Fencing Status: Active
```

#### `hadr quorum set-policy <policy-name>`

**Purpose:** Configure fencing policy (read-only in V0; [V1+] for mutation).

**Syntax:**

```bash
andromeda-cli hadr quorum set-policy <policy-name> [--dry-run] [--json]
```

**Supported Policies:**

| Policy | Behavior |
|--------|----------|
| `Epoch` | Requires new epoch strictly higher than previous. (DEC-020, default) |
| `Lease` | Requires lease renewal token from external coordinator. (Future) |
| `Split-Brain-Detector` | Integrates external gossip or heartbeat to detect partition. (Future) |

**Constraints:**

- **[V1+]** Policy changes are durable audit events.
- Fencing policy cannot be disabled without explicit authorization.

---

## JSON Output Format

All JSON output uses diagnostic format and is never used as the normative wire path. JSON diagnostics include:

1. **Contract metadata:** Command, parameters, validation status.
2. **Human-readable fields:** Role, state names (not raw enums).
3. **Timestamp:** Diagnostic output time (not persisted).
4. **Warning markers:** Fields that differ from the in-memory state.

**Example Diagnostic JSON:**

```json
{
  "_diagnostic": {
    "timestamp": "2026-01-15T14:30:45Z",
    "source": "andromeda-cli",
    "version": "1.0.0"
  },
  "command": "hadr status",
  "cluster_role": "Primary",
  "_warnings": [
    "cluster_role is a snapshot and may be stale"
  ]
}
```

---

## Error Handling

All commands return error code `0` on success and `1` on failure. Error messages are printed to stderr.

**Standard Error Format:**

```text
error: <error-code>: <human-readable message>
hint: <remediation or context>
```

**Example:**

```bash
$ andromeda-cli hadr promote 99
error: PromotionNotQuorumMember: Node 99 is not a registered quorum member
hint: Register node 99 first: andromeda-cli hadr node register 99 --dry-run
```

---

## Audit and Authorization

All HA/DR commands generate audit events that flow through the observability-forensic-architect's audit ledger (DEC-033). Mutations (register, deregister, promote, demote) require:

1. **Operator identity** (from mTLS certificate; DEC-018).
2. **Authorization grant** (UpdateClusterManifest, FenceNode, etc.).
3. **Fencing evidence** (external ticket, timestamp, approval).
4. **Audit record** (command, parameters, outcome, timestamp, identity).

---

## Related Documentation

- **DEC-020:** Quorum Runtime and Fencing Policy
- **DEC-024:** Promotion and Failover Boundary (F4)
- **DEC-027:** HA/DR Backup and Audit
- **DEC-033:** Durable Audit Ledger (Wave 12 Phase G)
- **TROUBLESHOOTING.md:** Failure scenarios and recovery runbooks

---

## Glossary

| Term | Meaning |
|------|---------|
| **Epoch** | Cluster membership generation counter; incremented on each promotion. |
| **LSN** | Log Sequence Number; byte offset in the write-ahead log. |
| **Durable LSN** | Highest LSN flushed to persistent storage at primary. |
| **Committed LSN** | Highest LSN visible to executing Procedures (consistent read point). |
| **Quorum Member** | Node that holds a vote in promotion elections. |
| **Quorum Size** | Minimum member count required for decision authority. |
| **Fencing Token** | Epoch-bound proof that a node is authorized as primary. |
| **Safe LSN** | Highest LSN durably persisted at a replica. |
| **Replica Lag** | Difference in bytes between primary durable_lsn and replica received_lsn. |
| **Promotion** | Transition of a replica to primary role. |
| **Failover** | Unplanned transition due to primary failure. |
| **Demote** | Graceful transition of primary to replica. |

---

## Versioning and Stability

- **Baseline:** V1.0.0 (Q1 2026)
- **Compatibility:** CLI contract is stable; subcommand additions will not remove existing parameters.
- **Breaking Changes:** Will be announced in release notes and documented in DEC-* records.
- **Diagnostic JSON:** Never used as runtime contract; format may change without notice.

