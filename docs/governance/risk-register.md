# Release Risk Register

## Purpose

Track current release-governance risks that must be resolved, deferred, or
explicitly scoped out before promotion.

## Risk Status

| Status | Meaning |
| --- | --- |
| `Blocking` | Release cannot claim the affected scope until the risk is closed or scope is removed. |
| `Open` | Risk needs owner review and evidence. |
| `Partial` | Some evidence exists, but it is advisory, isolated, or incomplete. |
| `Accepted` | Current-dated owner disposition accepts the residual risk for a narrowed scope. |
| `Closed` | Required evidence exists and no release-relevant uncertainty remains. |

## Current Risks

| Risk ID | Risk | Severity | Default status | Required disposition |
| --- | --- | --- | --- | --- |
| `GOV-001` | Release evidence is collected from a dirty or unattributed source state. | Critical | Blocking | Use a clean candidate and retain branch, commit, and `git status --short`. |
| `GOV-002` | Dependency or feature changes drift beyond the Rust baseline or supply-chain policy. | High | Blocking for affected release | Run locked workspace, advisory, license, source, and duplicate checks; review exceptions against `supply-chain-policy.md`. |
| `GOV-003` | Crash/recovery evidence is incomplete for a durable C5 path. | Critical | Blocking | Retain combined WAL, storage, transaction, execution, recovery, backup/PITR, and visibility evidence for the claimed scope. |
| `GOV-004` | Sustained fuzz evidence is missing for promoted byte, parser, protocol, ResultStream, or admission surfaces. | Critical | Blocking for affected surface | Run selected fuzz targets beyond smoke duration and retain corpus, logs, toolchain, and crash artifacts. |
| `GOV-005` | Miri or Loom evidence is missing for unsafe, memory-sensitive, or concurrency-sensitive changes. | High | Blocking for affected surface | Attach targeted evidence or scope the affected behavior out of the release claim. |
| `GOV-006` | Security, authorization, protocol, or audit checks are present only as isolated tests. | Critical | Blocking | Retain fail-closed admission, ContractHash rejection, protocol stability, and durable audit evidence together. |
| `GOV-007` | Evidence records omit command, date, toolchain, commit, artifact path, result, or residual risk. | High | Open | Recreate the record with complete metadata or rerun the gate. |

## Procedure

1. Open a risk entry when a gate is failed, skipped, partial, advisory-only, or
   missing.
2. Link the risk to one evidence record in `docs/testing/release-evidence-template.md`.
3. Assign an owner and review date.
4. Keep `Blocking` for any C5 durable truth, security, protocol, audit, backup,
   restore, PITR, HA/DR, fuzz, Miri, or Loom gap that remains in release scope.
5. Close only after retained evidence proves the exact candidate and scope.

## References

- `docs/governance/release-gates.md`
- `docs/governance/supply-chain-policy.md`
- `docs/testing/release-evidence-template.md`
