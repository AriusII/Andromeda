# Release Evidence Template

## Purpose

Record Step 11 validation evidence in a release-reviewable format.

Use one evidence record per command, crash/recovery drill, or manual acceptance decision. A release claim is incomplete when the exact command, date, toolchain, commit and branch, pass/fail result, artifact path, or residual risk is missing.

## Scope

This template covers validation evidence for:

- Roadmap Step 11 owner-suite gates.
- C5 combined gates.
- Crash/recovery drills.
- Fuzz, Miri, Loom, audit, security, backup/PITR, and HA/DR release gates.
- Manual release decisions that accept or reject residual risk.

## Non-goals

- Do not replace executable tests with this template.
- Do not use this template to promote benchmark, RAM, GPU, or temporary output to durable truth.
- Do not mark a failed, skipped, flaky, or partial gate as passing.
- Do not hide missing crash/recovery, audit, visibility, or protocol/security evidence in free-form notes.

## Prerequisites

- Run commands from the repository root.
- Capture the active branch and commit before running the command.
- Capture the Rust toolchain and relevant tool versions before or with the command.
- Store command output, logs, reports, or transcripts in a retained artifact path.
- Link each evidence record to the Step 11 roadmap label or crash/recovery scenario it supports.

## Procedure

1. Assign a stable evidence ID.
2. Record the exact command as executed.
3. Record the date in ISO 8601 format, including timezone or `Z` for UTC.
4. Record the toolchain, including `rustc -Vv`, `cargo -V`, and any gate-specific tool version such as `cargo nextest --version`, `cargo audit --version`, or `cargo deny --version`.
5. Record the current commit SHA and branch.
6. Record `Pass`, `Fail`, `Skipped`, or `Partial`.
7. Record the retained artifact path.
8. Record residual risk. Use `None recorded` only when the command passed and no release-relevant uncertainty remains.
9. Add the reviewer or owner and any follow-up issue or decision reference.

## Local Evidence Generator

Use `tools/testing/release_evidence.py` to capture local metadata and declared check results before assembling a retained release packet.

The generator:

- records Git branch, commit SHA, dirty worktree counts, platform, Python runtime, and Rust toolchain version metadata;
- accepts explicit check records with `pass`, `fail`, `skipped`, `gap`, or `partial` status;
- auto-detects existing local scripts under `tools/testing` as skipped checks without running them;
- emits text or JSON to stdout;
- does not run validation gates, write report files, modify the repository, or claim release readiness.

Example JSON capture:

```powershell
python -B tools/testing/release_evidence.py --json --check "id=fmt;status=pass;command=cargo fmt --all --check;artifact=artifacts/release/fmt.log;residual_risk=None recorded"
```

Use the schema in `tools/testing/release_evidence_schema.md` to validate field meaning before attaching the JSON output to a release packet.

## Evidence Record

| Field | Value |
| --- | --- |
| Evidence ID | `EV-YYYYMMDD-###` |
| Roadmap label or scenario | `tests/recovery`, `tests/storage`, `CR-11-WAL`, `CR-11-HADR`, or another Step 11 label |
| Command | Exact command, including package, test target, flags, environment variables, and arguments |
| Date | ISO 8601 date/time with timezone, for example `2026-05-08T14:30:00Z` |
| Toolchain | `rustc -Vv`; `cargo -V`; plus `cargo-nextest`, `cargo-audit`, `cargo-deny`, `cargo-fuzz`, Miri, or Loom version when used |
| Commit/branch | Full commit SHA and branch name |
| Pass/fail | `Pass`, `Fail`, `Skipped`, or `Partial` |
| Artifact path | Repository-relative or absolute retained artifact path |
| Residual risk | Remaining gap, uncertainty, skipped scope, flaky behavior, missing artifact, or `None recorded` |
| Owner/reviewer | Person, role, or release owner who reviewed this record |
| Follow-up reference | Issue, decision record, PR, or release checklist item |

## Command Evidence Table

| Evidence ID | Roadmap label or scenario | Command | Date | Toolchain | Commit/branch | Pass/fail | Artifact path | Residual risk | Owner/reviewer | Follow-up reference |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `EV-YYYYMMDD-001` | `CR-11-WAL` | `cargo test -p andromeda-wal --tests --locked` | `YYYY-MM-DDTHH:MM:SSZ` | `rustc ...; cargo ...` | `<commit-sha> on <branch>` | `Pass` | `artifacts/release/step-11/EV-YYYYMMDD-001.log` | `None recorded` | `<owner>` | `<issue-or-decision>` |

## Residual Risk Checklist

Record a residual risk when any of the following conditions apply:

- The command failed, was skipped, was flaky, or required retry.
- The command passed only a subset of a C5 gate.
- The artifact was not retained or cannot be reproduced.
- The command did not include crash/recovery coverage for a mission-critical path.
- The command did not prove visible commit after durable WAL.
- The command did not prove audit, authorization, or protocol admission behavior for accepted and rejected paths.
- The command did not prove backup/PITR restore boundaries or HA/DR quorum and fencing behavior.
- The evidence is analogous or advisory, such as statistics publication evidence used while Map publication coverage is still missing.

## Validation

Before release approval, verify that:

- Every Step 11 label with release impact has at least one evidence record.
- Every crash/recovery scenario in `step-11-validation-matrix.md` has a retained artifact or an explicit blocker.
- Every evidence record includes command, date, toolchain, commit/branch, pass/fail, artifact path, and residual risk.
- Failed, skipped, partial, or missing gates have follow-up references.
- Residual risks are reviewed by the release owner before any C5 readiness claim.

## Troubleshooting

If the command is too long for the table, keep the table entry concise and link to the full transcript in the artifact path.

If tool versions changed during a validation run, create separate evidence records for each toolchain.

If an artifact path points outside the repository, record the absolute path and retention owner.

If a command is replaced by a newer owner-suite command, keep the old record and add a new evidence record. Do not rewrite historical evidence.

## References

- `documentations/testing/step-11-validation-matrix.md`
- `tools/testing/release_evidence.py`
- `tools/testing/release_evidence_schema.md`
- `docs/codex/rust-critical-quality-gates.md`
- `docs/codex/mission-critical-change-policy.md`
- `documentations/governance/decisions/DEC-035-release-gate-chain.md`
- `documentations/governance/decisions/DEC-036-release-readiness-approval.md`
