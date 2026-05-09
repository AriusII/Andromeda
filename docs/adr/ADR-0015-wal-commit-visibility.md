# ADR-0015: WAL Commit Visibility Evidence

## Status

Accepted

## Purpose

Record the canonical Andromeda decision for WAL durability and commit
visibility.

Visible commit requires durable WAL evidence. A mutation, catalog change,
Procedure completion, or terminal transaction result is not visible until the
owning runtime can prove that the corresponding WAL record is covered by a
durable WAL prefix.

This ADR also defines the documentation contract for `DurableCommitEvidence`.
The contract is a cross-crate acceptance boundary for transaction, WAL, storage,
recovery, execution, observability, and audit work. It does not by itself change
runtime code or storage format behavior.

## Scope

This ADR applies to documentation and design acceptance for:

- transaction commit visibility;
- rollback terminal records;
- prepared mutation state;
- durable LSN reporting;
- WAL segment or generation identity;
- recovery replay boundaries;
- ResultStream terminal metadata;
- Procedure Store runtime records;
- audit, decision trace, and recovery trace projections.

The decision applies to application-visible Procedure results, catalog
publication, storage publication, and recovery-derived visibility. It also
applies when future implementation work introduces new commit-log, WAL, or
recovery evidence types.

## Non-goals

This ADR does not:

- define a new WAL binary format;
- change the FileWal disk format, page format, manifest format, or recovery
  algorithm;
- authorize visible commit before durable WAL evidence;
- authorize application-facing ad hoc SQL or bypass typed, cataloged Procedure
  contracts;
- treat audit records, RAM, HotStore, temp storage, benchmark output, or
  ResultStream metadata as database truth;
- claim that every runtime path already emits the final cross-crate evidence
  type;
- replace crash/recovery validation for storage, WAL, catalog, or transaction
  implementation changes.

## Prerequisites

Before using this ADR as acceptance evidence, reviewers must understand these
active decisions and invariants:

- `AGENTS.md` forbids making a commit visible before durable WAL.
- `docs/AGENTS.md` requires direct engineering documentation with validation
  and risk sections.
- ADR-0006 requires stronger validation for C4/C5 WAL, recovery, storage,
  security, catalog, and RPC changes.
- ADR-0011 keeps visible commit tied to durable WAL and preserves cold snapshot
  plus durable WAL as recovery truth.
- `docs/implementation/roadmap.md` states that no visible mutation exists
  without durable WAL and that reconstructible truth is the last valid cold
  snapshot plus durable WAL.
- `docs/testing/release-gates.md` treats visible commit without durable WAL
  as a release blocker and records explicit durable LSN evidence as active
  transaction work.
- `docs/adr/OPEN_DECISIONS.md` identifies WAL, commit visibility, and
  `DurableCommitEvidence` as a P0 ADR candidate.

## Decision

Andromeda accepts `DurableCommitEvidence` as the canonical documentation
contract for visible transaction terminal state. The implementation may use an
equivalent name only if the type, trace, and documentation clearly map to this
contract.

The minimum evidence fields are:

| Field | Requirement |
| --- | --- |
| Transaction identity | Identifies the transaction or Procedure invocation whose terminal state is being made visible or completed. |
| Terminal state | Distinguishes committed and rolled-back terminal records. A rollback terminal record is durable completion evidence, not visible mutation evidence. |
| Terminal record LSN | Identifies the WAL record that carries the commit or rollback terminal state. |
| Durable WAL boundary | Records the durable LSN or durable-prefix boundary observed by the WAL owner. The terminal record LSN must be less than or equal to this boundary. |
| WAL identity | Carries segment, generation, epoch, or equivalent identity sufficient to prevent mixing evidence from incompatible WAL histories. |
| Integrity evidence | Carries checksum, chain, or scan evidence sufficient for recovery and audit review to reject corrupt or truncated terminal records. |
| Source component | Names the component that observed or produced the durable boundary. Application, audit, and ResultStream layers may project this field but must not invent it. |

The ordering rule is strict:

1. Validate the Procedure contract and admission boundary before transaction
   creation.
2. Emit deterministic WAL records for mutation and terminal state.
3. Flush or otherwise prove the durable WAL prefix through the terminal record
   LSN.
4. Create `DurableCommitEvidence` from the WAL-owned durable boundary.
5. Publish committed state, completion metadata, Procedure Store records, audit
   projections, and decision traces only after the evidence exists.

Prepared state remains invisible until durable commit evidence exists. If the
terminal record is a rollback, the durable evidence proves rollback completion
and must not be interpreted as a visible commit.

Recovery must rebuild visibility only from the latest valid cold snapshot plus
durable WAL. Recovery must reject or quarantine terminal evidence that is
incomplete, truncated, corrupt, from the wrong WAL generation, outside the
durable prefix, or inconsistent with catalog or security admission evidence.

## Procedure

Use this ADR as the WAL commit visibility acceptance checklist.

1. Identify every path that can publish committed transaction state, catalog
   publication, Procedure completion, ResultStream terminal metadata, audit
   projection, or recovery-derived visibility.
2. Confirm that the path receives durable WAL evidence from the WAL owner or a
   validated recovery scan. Do not accept RAM-only state, audit-only records, or
   application-layer claims as durable evidence.
3. Confirm that the terminal record LSN is covered by the durable WAL boundary.
4. Confirm that prepared state stays invisible until the durable boundary covers
   the terminal commit record.
5. Confirm that rollback terminal evidence cannot be mistaken for visible
   mutation evidence.
6. Confirm that recovery replay derives visibility from cold snapshot plus
   durable WAL, not from cache, benchmark, GPU, audit, or ResultStream state.
7. Confirm that traces and audit records project the same durable boundary
   without becoming database truth.
8. For implementation changes, add focused tests that prove before-flush state
   is invisible, after-flush committed state is visible, rollback completion is
   durable but not visible mutation, and recovery rejects incomplete terminal
   evidence.

## Validation

For this documentation-only ADR, validation is textual and structural:

- Confirm that the ADR states visible commit requires durable WAL evidence.
- Confirm that the ADR keeps audit, RAM, HotStore, temp storage, benchmark
  output, and ResultStream metadata out of the database truth boundary.
- Confirm that the ADR does not change WAL, page, manifest, or recovery binary
  formats.
- Confirm that the ADR names future implementation gates instead of claiming
  runtime behavior changed.

Implementation changes that affect this ADR require stronger validation:

- transaction state tests for pre-durable and post-durable visibility;
- WAL durability fence tests;
- recovery replay tests for valid, truncated, corrupt, and wrong-generation
  terminal records;
- catalog publication tests when catalog visibility is affected;
- audit and trace projection tests proving evidence is projected but not treated
  as truth;
- crash/recovery tests for every path that can affect visible commit.

Rust builds are not required for this ADR alone because the accepted artifact is
documentation-only. Any code, codec, WAL, recovery, storage, catalog, or
security change must cite its owning build, test, fuzz, property, Miri, or
crash/recovery evidence separately.

## Risks

- Implementations can drift by using a local durable LSN value that did not come
  from the WAL owner or a validated recovery scan.
- Audit or ResultStream metadata can be overstated as database truth if
  reviewers do not preserve the cold snapshot plus durable WAL boundary.
- Rollback evidence can be confused with visible commit evidence unless terminal
  state is explicit.
- Recovery can accept incomplete terminal records unless the durable prefix,
  checksum or chain evidence, and WAL identity are validated together.
- Cross-crate naming can drift if future code introduces equivalent evidence
  types without documenting their field-level mapping to this ADR.

## Troubleshooting

Use this section when reviewing WAL, transaction, recovery, or observability
documentation.

| Symptom | Corrective action |
| --- | --- |
| A document says commit is visible after an in-memory state change. | Replace the claim with visible commit requires durable WAL evidence. |
| A document treats audit records as proof of database truth. | Reword audit as forensic or authorization evidence and cite cold snapshot plus durable WAL as truth. |
| A document omits durable LSN or durable-prefix evidence from terminal metadata. | Require `DurableCommitEvidence` or an explicitly mapped equivalent. |
| A document lets rollback evidence imply visible mutation. | Separate rollback completion evidence from committed visibility evidence. |
| A recovery design reuses cache, ResultStream, benchmark, or GPU output as truth. | Reject the design and require recovery from latest valid cold snapshot plus durable WAL. |
| A future implementation changes WAL, page, manifest, or recovery formats under this ADR. | Require a separate format or recovery ADR and crash/recovery validation. |

## References

- `AGENTS.md`
- `docs/AGENTS.md`
- `docs/adr/ADR-0006-mission-critical-validation.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `docs/implementation/roadmap.md`
- `docs/implementation/roadmap.md`
- `docs/adr/OPEN_DECISIONS.md`
- `docs/runbooks/wal-pressure.md`
- `docs/specs/storage-wal.md`
- `docs/specs/transaction-recovery.md`
- `docs/specs/catalog-srpl.md`
