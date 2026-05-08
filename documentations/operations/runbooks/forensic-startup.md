# Forensic Startup Runbook

## Purpose

Use this runbook when startup, recovery, storage validation, WAL validation, manifest validation, catalog validation, audit validation, or cluster evidence requires inspection without trusting the database as safe for normal application traffic.

## Scope

This runbook covers FastStart, SafeStart, ForensicStart, RefuseStart, RestoreValidation routing, application traffic blocking, non-mutating inspection, evidence preservation, `RecoveryReport` or `ForensicReport` production, and exit criteria.

## Non-goals

This runbook does not repair corruption by overwriting evidence, replay past a corruption boundary, publish a manifest from forensic inspection, rebuild indexes into durable truth, re-authorize application work, or use audit replay as database truth. It does not define a new forensic command surface.

## Prerequisites

- Operators can prevent Application Surface traffic from opening against the affected database.
- Administration Surface or documented recovery control access is available for startup-mode selection and evidence capture.
- WAL segments, manifests, cold snapshots, catalog metadata, page or segment validation evidence, audit ledger evidence, and cluster-state evidence are available or explicitly recorded as missing.
- A writable evidence sink exists outside the inspected database truth, or an isolated audit path is available for forensic report output.
- Restore/PITR routing is available when local artifacts cannot establish safe truth.
- The incident owner can place a forensic hold on WAL, audit, snapshot, manifest, trace, backup, and cluster artifacts.

## Application Surface Separation

Forensic startup is not an Application Surface capability. The Application Surface must reject forensic startup, forensic hold management, raw page inspection, raw WAL inspection, corruption-boundary investigation, and recovery inspection with a typed denial and `NoTransaction` effect.

`ForensicStart` blocks Application Surface traffic. It must also block transaction creation, catalog publication, HA/DR promotion, manifest switching, checkpoint publication, index rebuild publication, Map refresh publication, and statistics publication unless a separate Administration Surface policy explicitly authorizes a bounded operation outside the inspected database truth.

## Status

| Area | Status | Evidence or limitation |
| --- | --- | --- |
| Startup mode contract | Contract preview. | `documentations/specs/RecoveryReport_v0.md` defines FastStart, SafeStart, ForensicStart, RestoreValidation, and RefuseStart behavior. |
| ForensicStart doctrine | Contract preview. | `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md` requires application blocking and a consistency report. |
| File WAL recovery evidence | Implemented durable behavior where covered by file WAL and recovery tests. | `documentations/testing/step-11-validation-matrix.md`; existing recovery runbooks. |
| Incident-grade ForensicStart drill | Planned gap unless a release record attaches retained forensic artifacts and application-surface denial evidence. | `documentations/testing/step-11-validation-matrix.md`. |
| Forensic audit evidence | Contract preview and implementation evidence where covered by AuditLedger contracts. | `documentations/specs/AuditLedger_v0.md`; `documentations/governance/decisions/DEC-033-durable-audit-ledger.md`. |

## Startup Mode Decision

| Requested mode | When to use | Application traffic | Mutation policy |
| --- | --- | --- | --- |
| `FastStart` | Clean startup with only minimal gates required by policy. | Allowed only after gates pass. | Replay valid WAL needed for visibility. |
| `SafeStart` | Normal recovery with reinforced checks after crash, shutdown uncertainty, or operational risk. | Allowed only after reinforced gates pass. | Replay valid WAL and validate stronger catalog, manifest, and storage invariants. |
| `ForensicStart` | Corruption suspicion, unknown format, unsafe replay boundary, audit gap, cluster anomaly, or incident investigation. | Blocked. | Inspection-only; no mutation of inspected database truth. |
| `RestoreValidation` | Backup or PITR restore needs isolated validation before opening. | Blocked until restore is accepted. | Replay from selected snapshot and WAL archive into an isolated target. |
| `RefuseStart` | Startup cannot safely inspect, recover, or report. | Blocked. | None. |

## Findings Ordered By Severity

| Severity | Finding | Exact invariant or contract affected | Concrete remediation |
| --- | --- | --- | --- |
| Critical | ForensicStart must never mutate inspected database truth. | RecoveryReport non-mutating forensic constraints; invariant 10. | Block redo publication, undo mutation, manifest switching, index publication, Map publication, and application traffic. |
| Critical | FastStart or SafeStart must not silently accept evidence that requires forensic handling. | RecoveryReport corruption and truncation classification. | Downgrade to ForensicStart or RefuseStart when WAL, manifest, snapshot, catalog, page, or format gates fail. |
| Critical | Forensic operations must not be reachable through Application Surface. | SecurityAdmission surface classification and invariant 8. | Route through Administration or recovery control paths only and record typed application denials. |
| High | A forensic report without source artifact references is not useful evidence. | RecoveryReport required report shape. | Include bounded identifiers for WAL, manifests, snapshots, catalog versions, storage fingerprints, audit ids, and corruption boundaries. |
| High | Audit replay explains decisions but does not reconstruct database truth. | AuditLedger forensic replay contract. | Use audit as evidence only; establish truth from the latest valid cold snapshot plus durable WAL. |

## Evidence Artifacts

Retain the following artifacts before and during forensic startup:

| Artifact | Required contents |
| --- | --- |
| Startup request | Requested mode, effective mode, operator, incident id, generated-at timestamp, and refusal or downgrade reason. |
| Application-block evidence | Application Surface traffic state, denial ids, attempted forbidden operation families, and `NoTransaction` effect evidence. |
| WAL evidence | Segment range, first inspected LSN, last readable LSN, last valid LSN, chain validation, checksum validation, and corruption boundary. |
| Manifest and snapshot evidence | Active manifest, previous manifest when used, cold snapshot id, snapshot hash or checksum, and root-pointer validation. |
| Storage evidence | Page, segment, format fingerprint, trailer, block hash, and page LSN validation results where applicable. |
| Catalog and security evidence | Catalog version, Procedure contract status, dependency validation, security policy status, and admission policy status. |
| Derived structure evidence | Index and Map consistency policy results, with source table health proven before derived validation is trusted. |
| Audit and trace evidence | `RecoveryTrace` ids, durable audit event ids, forensic hold id, operator request id, and report sink id. |
| Outcome evidence | Final outcome, visibility decision, degraded status, required operator action, and restore/PITR routing decision. |

## Procedure

1. Select the startup mode.
   - Use `SafeStart` for ordinary crash recovery when stronger validation is required but evidence is compatible.
   - Use `ForensicStart` when there is suspected corruption, unknown format, unsafe replay, audit gap, cluster anomaly, or policy uncertainty.
   - Use `RefuseStart` when the system cannot safely inspect artifacts or emit bounded evidence.

2. Block application traffic.
   - Ensure Application Surface sessions cannot create transactions, invoke Procedures, perform catalog reads that imply normal availability, or call forensic controls.
   - Record typed denial evidence for forbidden operation attempts.
   - Keep Administration Surface access restricted to incident operators.

3. Place forensic hold.
   - Preserve WAL segments, audit ledger records, cold snapshots, manifests, cluster manifests, trace files, backup manifests, and relevant external storage artifacts.
   - Stop compaction, truncation, retention deletion, snapshot cleanup, and WAL garbage collection for held artifacts.
   - Record the hold id and scope.

4. Inspect durable sources without mutation.
   - Validate WAL length, checksum, predecessor chain, and gapless LSN order.
   - Validate manifest hashes, root pointers, generations, and storage format fingerprints.
   - Validate cold snapshot binding and page or segment checksums where applicable.
   - Validate catalog object versions, dependencies, Procedure contract compatibility, and security policy status.
   - Inspect indexes, Maps, and statistics only as derived evidence after source table health is proven.

5. Classify boundaries.
   - Record tail truncation separately from middle corruption.
   - Record the earliest boundary that prevents safe replay.
   - Record unknown or unsupported storage formats as mutation blockers.
   - Treat missing audit evidence as an incident finding, not as permission to proceed silently.

6. Produce the report.
   - Emit `RecoveryReport` or `ForensicReport` evidence to an allowed external report sink or isolated audit path.
   - Include requested mode, effective mode, source artifacts, WAL boundaries, replay evidence, skipped records, corruption evidence, validation evidence, audit links, outcome, and operator action.
   - Do not include secrets, private keys, raw credentials, unbounded payload bytes, full page bodies, or full WAL bodies.

7. Decide the outcome.
   - Use `RecoveredOnline` only when recovery completed, all required invariants passed, and policy allows normal opening.
   - Use `RecoveredReadOnly` when evidence is safe for inspection but policy blocks writes.
   - Use `RecoveredDegraded` only with explicit policy and traceable operator decision.
   - Use `ForensicOnly` when normal recovery is unsafe or not authorized.
   - Use `RestoreRequired` when local artifacts cannot establish safe truth.
   - Use `Refused` when startup failed before safe inspection or recovery could complete.

8. Exit forensic mode.
   - Keep Application Surface traffic blocked until the outcome permits reopening.
   - Release forensic hold only after incident authority approves and backup/PITR, audit, WAL, and compliance retention gates are still satisfied.
   - Route to the restore/PITR drill runbook when the outcome is `RestoreRequired`.

## Validation

For documentation-only changes to this runbook, validate with:

```powershell
git diff -- documentations/operations/runbooks/forensic-startup.md documentations/operations/runbooks/index.md
rg -n "Application Surface|ForensicStart|RecoveryReport|ForensicReport|NoTransaction|WAL|snapshot|ad hoc SQL|gRPC" documentations/operations/runbooks/forensic-startup.md
```

Required before accepting runtime work related to this runbook:

- FastStart, SafeStart, ForensicStart, RestoreValidation, and RefuseStart mode tests.
- Application Surface denial tests for forensic controls and raw evidence inspection.
- WAL tail truncation, middle corruption, chain break, duplicate LSN, and checksum failure tests.
- Unknown storage format tests proving normal startup refuses mutation and ForensicStart remains read-only.
- Manifest, snapshot, page, segment, catalog, security, index, and Map validation tests.
- Report-shape tests proving bounded `RecoveryReport` or `ForensicReport` evidence is emitted.
- Forensic hold tests proving WAL, audit, snapshot, manifest, and trace artifacts are retained.
- Restore routing tests when local truth cannot be proven.

Suggested commands when related code changes exist:

```powershell
cargo test -p andromeda-storage --locked forensic_start -- --nocapture
cargo test -p andromeda-storage --test recovery_contract --locked -- --nocapture
cargo test -p andromeda-storage --test file_wal_recovery_contract --locked -- --nocapture
```

## Rollback

| Phase | Rollback or containment action |
| --- | --- |
| Before inspection starts | Cancel the startup attempt, keep the database closed, and retain the request evidence. |
| During ForensicStart inspection | Stop inspection, keep forensic hold active, and preserve partial report evidence; do not mutate source artifacts to "undo" the attempt. |
| After report emission | If the report is wrong or incomplete, supersede it with a new report id and cross-reference the flawed report; do not rewrite the original evidence. |
| After accidental mutation | Treat the database as contaminated, keep application traffic blocked, preserve the mutation evidence, and route to restore/PITR or incident review. |
| After report sink failure | Keep startup refused or forensic-only until an allowed evidence sink is available or incident authority records the residual risk. |

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| ForensicStart attempts WAL redo or manifest publication. | Forensic mode is mutating inspected truth. | Stop the attempt, preserve evidence, and fix the recovery path before reopening. |
| Application clients can inspect raw WAL or pages. | Surface classification drift. | Reject the route on the Application Surface and require Administration or recovery control authorization. |
| Report says audit replay recovered state. | Audit evidence is being confused with database truth. | Reword or fix the path so truth comes from cold snapshot plus durable WAL only. |
| SafeStart opens despite unknown format evidence. | Storage format gate is bypassed. | Refuse normal startup and allow only read-only forensic inspection when policy permits. |
| Forensic hold blocks retention cleanup longer than expected. | Incident hold is still active or not scoped. | Keep held artifacts, refine hold scope through incident authority, and release only after retention gates pass. |

## References

- `AGENTS.md`
- `.agents/instructions/TRANSACTION_RECOVERY_STANDARD.md`
- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`
- `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md`
- `documentations/specs/AuditLedger_v0.md`
- `documentations/specs/RecoveryReport_v0.md`
- `documentations/specs/SecurityAdmissionCanonicalOrder_v0.md`
- `documentations/testing/step-11-validation-matrix.md`
- `documentations/operations/runbooks/corruption-suspicion.md`
- `documentations/operations/runbooks/restore-pitr-drill.md`
