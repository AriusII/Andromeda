# Release Evidence Template

## Purpose

Use this template for every release-significant command, crash/recovery drill,
fuzz run, Miri run, Loom run, skipped gate, partial gate, or manual release
decision.

## Evidence Record

| Field | Value |
| --- | --- |
| Evidence ID | `EV-YYYYMMDD-###` |
| Gate or scenario | Workspace, supply-chain, recovery, security, protocol, backup/PITR, HA/DR, fuzz, Miri, Loom, or other named gate |
| Command | Exact command, including package, target, profile, flags, environment notes, and arguments |
| Working directory | Repository root unless another path is recorded |
| Date | ISO 8601 date and timezone |
| Toolchain | `rustc -Vv`; `cargo -V`; plus gate-specific tool versions |
| Source state | Full commit SHA, branch, and `git status --short` summary |
| Result | `Pass`, `Fail`, `Skipped`, or `Partial` |
| Artifact path | Repository-relative or absolute retained artifact path |
| Residual risk | Remaining gap, skipped scope, unsupported operation, flaky behavior, missing artifact, or `None recorded` |
| Reviewer | Person or role that reviewed the evidence |
| Follow-up | Issue, decision, PR, or release checklist reference |

## Command Evidence Table

| Evidence ID | Gate or scenario | Command | Date | Toolchain | Source state | Result | Artifact path | Residual risk | Reviewer | Follow-up |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `EV-YYYYMMDD-001` | `CR-WAL` | `cargo test -p andromeda-wal --tests --locked` | `YYYY-MM-DDTHH:MM:SSZ` | `rustc ...; cargo ...` | `<commit> on <branch>` | `Pass` | `artifacts/release/EV-YYYYMMDD-001.log` | `None recorded` | `<owner>` | `<link>` |

## Residual Risk Checklist

Record residual risk when:

- the command failed, was skipped, was partial, or required retry;
- the artifact was not retained;
- the command was narrower than the release claim;
- crash/recovery, durable visibility, audit, authorization, protocol, backup,
  restore, PITR, HA/DR, fuzz, Miri, or Loom evidence is missing for the affected
  scope;
- the evidence is advisory, such as benchmark output, fuzz smoke, Miri
  inventory, or a standalone model that does not cover production integration.

## Validation

Before approval, verify that every release-significant command and decision has
a complete evidence record. Missing metadata converts the gate to `Partial` or
`Skipped`; it does not pass by implication.
