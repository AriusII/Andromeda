# HA/DR Failover Runbook

## Purpose

Use this runbook to perform or rehearse an Andromeda HA/DR failover while preserving the single-primary invariant, durable WAL truth, quorum evidence, fencing evidence, and application-surface separation.

## Scope

This runbook covers planned primary demotion, unplanned primary suspicion, replica promotion eligibility, quorum verification, fencing, recovery to a consistent LSN, cluster manifest publication, replica repointing, and controlled reopening of application traffic.

## Non-goals

This runbook does not define a multi-primary mode, automatic failover policy, new CLI syntax, new RPC endpoints, or new storage formats. It does not treat a replica as a backup. It does not permit promotion when quorum, fencing, durable LSN eligibility, or recovery evidence is missing.

## Prerequisites

- Operators have HA/DR Cluster Surface access for cluster operations and Administration Surface access for incident coordination.
- Application clients cannot invoke failover, promotion, quorum, fencing, WAL shipping, membership, or replica-control operations.
- The current cluster membership, `MembershipEpoch`, primary node id, replica ids, quorum members, and fencing policy are known.
- Each candidate replica reports durable LSN, applied LSN, replication lag, health state, and divergence status from durable or auditable evidence.
- WAL retention is protected for the promotion window, lagging replicas, backup/PITR requirements, and any forensic hold.
- Audit and trace sinks are available for HA/DR decisions, recovery decisions, and surface-admission denials.
- A restore/PITR route exists if no replica can prove safe promotion.
- The local HA/DR drill checker is understood as simulation-only. It does not start nodes, open sockets, publish cluster manifests, or fence a real primary.

## Application Surface Separation

Failover is never an Application Surface operation. The Application Surface may only carry typed, cataloged Procedure invocation and allowed contract reads. It must reject Administration, backup, restore, PITR, quorum, fencing, membership, promotion, demotion, WAL shipping, and replica-control requests with a typed denial and `NoTransaction` effect.

During failover, application traffic may be paused, rejected, or reopened by policy, but the control decisions must be made through the HA/DR Cluster Surface or Administration Surface. Do not tunnel cluster commands through application RPC, Procedure contracts, generic command payloads, or ad hoc SQL.

## Status

| Area | Status | Evidence or limitation |
| --- | --- | --- |
| Single-primary HA/DR doctrine | Contract preview. | `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md` defines V0 as Single Primary plus Replicas and rejects multi-primary. |
| Majority quorum, membership epoch, and LSN ranking | Contract preview with standard-level doctrine. | `.agents/instructions/TRANSACTION_RECOVERY_STANDARD.md`. |
| Promotion and HA/DR owner tests | Implemented durable behavior where covered by named owner tests. | `documentations/testing/step-11-validation-matrix.md` lists HA/DR membership, quorum, promotion, WAL shipping, and stream-mapping tests. |
| Full production failover drill | Planned gap until release evidence includes cluster simulation, fencing proof under partition, and failover-with-recovery transcript. | `documentations/testing/step-11-validation-matrix.md`. |
| HA/DR audit evidence | Contract preview and implementation evidence where covered by audit contracts. | `documentations/specs/AuditLedger_v0.md`; `documentations/governance/decisions/DEC-027-hadr-backup-audit.md`. |

## Findings Ordered By Severity

| Severity | Finding | Exact invariant or contract affected | Concrete remediation |
| --- | --- | --- | --- |
| Critical | A failover without proven fencing can create split brain. | HA/DR V0 single-primary topology; membership epoch; fencing contract. | Abort promotion unless the old primary is fenced or otherwise proven unable to accept writes for the candidate epoch. |
| Critical | A replica with stale, divergent, or unproven durable LSN must not be promoted. | Commit visible equals WAL durable; LSN ranking; promotion eligibility. | Promote only a quorum-eligible candidate whose safe LSN satisfies the required target and whose WAL chain is non-divergent. |
| Critical | Failover must not be operated from the Application Surface. | Invariant 8 and SecurityAdmission surface classification. | Route failover through HA/DR Cluster Surface or Administration Surface only and record application-surface denial evidence. |
| High | A failover that lacks trace evidence is operationally dangerous even if it appears successful. | AuditLedger HA/DR decision family; RecoveryReport evidence contract. | Preserve quorum votes, fencing decisions, candidate ranking, target LSN, recovery report, cluster manifest update, and operator approval. |
| High | WAL truncation during failover can destroy the promotion or PITR window. | WAL retention boundaries for replicas, backups/PITR, and forensic hold. | Suspend WAL reclamation until promotion, replica catch-up, backup/PITR, and forensic gates clear. |

## Evidence Artifacts

Retain the following artifacts for every planned or unplanned failover:

| Artifact | Required contents |
| --- | --- |
| Incident or change id | Operator, approver, start time, failover type, RPO target, RTO target, and reason. |
| Cluster-state snapshot | Primary id, candidate replicas, `MembershipEpoch`, quorum members, health states, and last-seen times. |
| WAL and LSN evidence | Primary durable LSN, candidate durable LSN, candidate applied LSN, required target LSN, WAL chain validation, and lag calculation. |
| Quorum evidence | Vote set, quorum rule, candidate eligibility result, and rejected candidates with reasons. |
| Fencing evidence | Fencing token or proof, fenced node id, fenced epoch, time, authority, and failure mode if fencing is partial. |
| Recovery evidence | Startup mode, target LSN, `RecoveryReport` id, replay or validation status, and any corruption boundary. |
| Manifest evidence | Previous cluster manifest, new cluster manifest, publication time, and membership epoch transition. |
| Audit evidence | HA/DR decision audit ids, recovery audit ids, operator request id, and application-surface denial ids. |
| Reopen evidence | Application traffic state, read-only or read-write opening decision, and post-failover health checks. |

## Procedure

1. Classify the failover.
   - Use planned failover for maintenance when the primary can participate.
   - Use unplanned failover only when the primary is suspected failed, isolated, or unsafe.
   - Record the incident id, RPO target, RTO target, current primary, candidate set, and whether the old primary can still communicate.

2. Contain application traffic.
   - Stop accepting new write Procedure invocations or force them through the documented rejection path.
   - Keep Application Surface sessions from invoking HA/DR, Administration, backup, restore, PITR, or forensic operations.
   - Preserve surface-admission denial evidence for any attempted application-side control operation.

3. Preserve WAL and cluster evidence.
   - Freeze WAL reclamation for the affected database and cluster epoch.
   - Capture cluster membership, quorum membership, replica health, durable LSNs, applied LSNs, and WAL shipping acknowledgments.
   - Capture current audit-ledger and recovery-trace identifiers.

4. Verify primary suspicion.
   - Confirm whether the primary is alive, partitioned, unhealthy, or voluntarily demoting.
   - Reject unplanned promotion if evidence only shows transient client failure and quorum cannot distinguish the primary state.
   - For planned failover, stop primary writes through the approved control path before ranking candidates.

5. Rank promotion candidates.
   - Require candidate membership in the voting or eligible cluster set.
   - Require candidate health to be acceptable under policy.
   - Require no known WAL divergence.
   - Prefer the candidate with the highest valid durable LSN that satisfies the required target.
   - Reject candidates with lag or divergence incompatible with the declared RPO.

6. Verify quorum.
   - Evaluate the quorum rule for the current membership epoch.
   - Record each vote and rejection reason.
   - Abort if majority or the configured quorum rule is not met.
   - Do not substitute application-client reachability for quorum evidence.

7. Fence the old primary.
   - Acquire or verify fencing for the suspect primary and epoch.
   - Confirm the old primary cannot accept writes for the promoted epoch.
   - Abort promotion if fencing is missing, stale, ambiguous, or contradicted by health evidence.

8. Recover the candidate to a consistent LSN.
   - Validate the WAL chain and required target LSN on the candidate.
   - Use SafeStart when the candidate evidence is healthy and policy permits.
   - Use ForensicStart or restore routing when corruption, unknown format, or unsafe replay evidence appears.
   - Produce `RecoveryReport` evidence before promotion visibility.

9. Promote the candidate.
   - Publish the new primary identity only after quorum, fencing, LSN, and recovery gates pass.
   - Increment or validate the `MembershipEpoch`.
   - Publish the cluster manifest through the HA/DR Cluster Surface control path.
   - Emit HA/DR audit evidence for the promotion decision and manifest transition.

10. Repoint replicas.
    - Direct replicas to follow the new primary and epoch.
    - Reject replicas that report stale epoch, divergent WAL, or unsafe format evidence.
    - Keep WAL retention until required replicas, backup/PITR windows, and forensic holds are safe.

11. Reopen application traffic.
    - Start in read-only or degraded mode when policy requires extra observation.
    - Reopen writes only after the new primary is authoritative, recovery evidence passes, and admission policy permits.
    - Record the reopen decision, traffic mode, residual lag, and remaining operator actions.

12. Run the local HA/DR cluster drill check.
    - Execute the simulation-only checker from the repository root.
    - Use JSON output when a local report, CI job, or release checklist needs structured evidence.
    - Treat a passing result as local artifact coverage only. It does not prove that a real cluster can fail over safely.
    - Convert missing tests, runbooks, or evidence commands into follow-up work before claiming HA/DR readiness.

## Validation

For documentation-only changes to this runbook, validate with:

```powershell
git diff -- documentations/operations/runbooks/hadr-failover.md documentations/operations/runbooks/index.md
rg -n "Application Surface|HA/DR Cluster Surface|quorum|fencing|WAL|RecoveryReport|ad hoc SQL|gRPC" documentations/operations/runbooks/hadr-failover.md
python tools/testing/hadr_cluster_drill_check.py
python tools/testing/hadr_cluster_drill_check.py --json
```

Required before accepting runtime work related to this runbook:

- Promotion eligibility tests rejecting non-member, stale, lagging, divergent, and unfenced candidates.
- Quorum membership and membership-epoch tests.
- Fencing tests under network partition and stale primary recovery.
- WAL shipping reclaimability tests that protect required promotion and PITR windows.
- Recovery tests proving candidate startup cannot publish unsafe state.
- HA/DR stream-mapping tests proving cluster operations are not reachable through the Application Surface.
- Audit tests proving HA/DR decisions produce durable, correlated evidence.
- A recorded cluster failover drill with primary crash, partition, fencing proof, candidate recovery, promotion, manifest update, and replica repointing.

Suggested commands when related code changes exist:

```powershell
cargo test -p andromeda-storage --test hadr_promotion_runtime_contract --locked -- --nocapture
cargo test -p andromeda-storage --test hadr_membership_store_contract --locked -- --nocapture
cargo test -p andromeda-storage --test quorum_membership_contract --locked -- --nocapture
cargo test -p andromeda-storage --test wal_shipping_reclaimability_contract --locked -- --nocapture
cargo test -p andromeda-quic --test hadr_stream_mapping_contract --locked -- --nocapture
cargo test -p andromeda-observe --test hadr_backup_audit_contract --locked -- --nocapture
```

## Rollback

| Phase | Rollback or containment action |
| --- | --- |
| Before fencing | Cancel the failover attempt, keep the current primary authoritative, resume traffic only after health evidence passes, and retain the aborted attempt evidence. |
| After fencing but before promotion | Keep the cluster read-only or unavailable, select another eligible candidate or restore primary health, and do not unfence based on stale evidence. |
| After promotion manifest publication | Do not revive the old primary as primary. Rejoin it only as a replica after it acknowledges the new epoch and passes WAL divergence checks, or run a separate planned failover back. |
| After detected divergence | Keep application traffic blocked or read-only, preserve WAL and audit artifacts, and route to restore/PITR if no safe cluster state can be proven. |
| After audit or report failure | Treat the operation as incomplete. Keep the cluster in the safest available mode until durable evidence is reconstructed or an incident authority accepts the residual risk. |

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Candidate has highest LSN but is not a quorum member. | Membership and LSN ranking are being confused. | Reject the candidate until membership is corrected through the approved HA/DR path. |
| Fencing token is missing or references an older epoch. | Stale or incomplete fencing operation. | Abort promotion and preserve split-brain evidence for incident review. |
| Application traffic can issue a promotion request. | Surface separation drift. | Disable the route, require typed denial with `NoTransaction`, and add surface-admission evidence. |
| Replica reports a higher applied LSN than the primary durable LSN. | Divergence, stale metrics, or untrusted telemetry. | Validate durable WAL chain evidence before using the replica for any promotion decision. |
| Failover succeeds but audit evidence is missing. | HA/DR decision events were not persisted or correlated. | Treat readiness as unproven and require audit remediation before production HA/DR claims. |
| `hadr_cluster_drill_check.py` passes but no real partition test exists. | The local checker only verifies repository evidence and documented commands. | Keep production HA/DR readiness blocked until a recorded cluster simulation proves quorum, fencing, recovery, promotion, and replica repointing. |
| `hadr_cluster_drill_check.py` reports a missing evidence command. | A runbook or validation matrix no longer records the local test target. | Restore the documented command only after confirming the crate test still exists, or record the missing target as a planned gap. |

## References

- `AGENTS.md`
- `.agents/instructions/TRANSACTION_RECOVERY_STANDARD.md`
- `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md`
- `documentations/specs/AuditLedger_v0.md`
- `documentations/specs/RecoveryReport_v0.md`
- `documentations/specs/SecurityAdmissionCanonicalOrder_v0.md`
- `documentations/testing/step-11-validation-matrix.md`
- `documentations/governance/decisions/DEC-024-promotion-failover-boundary.md`
- `documentations/governance/decisions/DEC-027-hadr-backup-audit.md`
