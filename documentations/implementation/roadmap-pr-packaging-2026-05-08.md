# Andromeda Roadmap PR Packaging Guide - 2026-05-08

## Purpose

Prepare reviewable pull-request and release packages for the 2026-05-08
roadmap wave without changing the current Git index.

Use this guide to split the dirty branch into coherent review lots, preserve
owner boundaries, and record the validation needed before any release claim.
The branch is dirty. No automatic staging, commit, reset, restore, checkout,
clean, or broad formatting command is allowed from this packaging guide.

## Scope

This guide applies to `C:/Users/Arius/RustroverProjects/Andromeda` as a local
dirty worktree on 2026-05-08.

It covers packaging triage for these review lots:

- Workspace and crates.
- Specifications and implementation documentation.
- Storage, WAL, transaction, and recovery.
- Catalog, contracts, DefinitionBatch, and Procedure Store.
- SRPL language, parser, binder, IR, lowering, and diagnostics.
- RPC, QUIC, security admission, IAM, audit, and surface separation.
- Testing, fuzz, Miri, Loom, and release gate evidence.
- Runbooks, governance, ADRs, risk, and release readiness.

The guide is documentation-only. It describes how to sort work; it does not
stage, commit, approve, or reject any candidate package.

## Non-goals

This guide does not:

- Stage, unstage, commit, amend, reset, restore, checkout, clean, or push
  anything.
- Authorize a broad `git add .`, `git add -A`, `git commit`, or directory-level
  commit from the repository root.
- Resolve another worker's `MM`, `AD`, modified, added, deleted, or untracked
  paths.
- Treat the current dirty branch as a clean release candidate.
- Replace owner review, ADR acceptance, DEC acceptance, release approval, or
  retained command evidence.
- Promote specifications, runbooks, benchmark output, ScenarioEvidence, audit
  output, RAM state, temporary state, or GPU output to database truth.
- Claim C4 or C5 readiness without crash/recovery, security, audit, protocol,
  property, fuzz, Miri, Loom, or release evidence as applicable.

## Prerequisites

Before any PR or release package is prepared, complete these checks:

1. Confirm the branch, commit, and dirty state with read-only commands.
2. Assign one owner for every path in the candidate package.
3. Inspect staged and unstaged content separately for every path in the package.
4. Treat the current index as contaminated by unrelated worker output until the
   package owner proves otherwise.
5. Stop if a package includes an unresolved `MM` or `AD` path.
6. Capture exact validation commands, date, toolchain, commit, branch, pass or
   fail result, artifact path, and residual risk before any release claim.

Read-only commands that are safe for triage:

```powershell
git status --short --branch
git status --porcelain=v1 -- <pathspecs>
git diff --name-status -- <pathspecs>
git diff --cached --name-status -- <pathspecs>
git diff --check -- <pathspecs>
git diff --cached --check -- <pathspecs>
```

Do not run a staging or commit command from automation. The release owner must
make explicit manual staging and commit decisions after path ownership and
staged-versus-working reconciliation are complete.

## Procedure

Use this procedure to turn the roadmap wave into reviewable PR packages.

### 1. Freeze the candidate view

Record the current state before choosing package boundaries.

Required evidence:

- Branch and commit.
- `git status --short --branch`.
- Candidate pathspec list.
- Owner for each pathspec.
- Known `MM`, `AD`, modified-only, staged-only, and untracked entries in the
  candidate pathspecs.

Decision rule:

- If a path has staged and unstaged divergence, inspect both sides and require
  the path owner to decide the final file state.
- If a path is `AD`, require the owner to choose present, deleted, or moved as
  the final package state.
- If an untracked directory is included, enumerate files explicitly before
  package review.

### 2. Split by review lot

Use the following lots as the first-pass PR packaging map. A lot can become
more than one PR when the path count, risk class, or validation cost is too
large.

| Lot | Review purpose | Candidate path groups | Minimum package gate |
| --- | --- | --- | --- |
| 01 - Workspace and crates | Review workspace shape, crate topology, toolchain files, and new low-level scaffold crates without mixing domain behavior. | `Cargo.toml`, `Cargo.lock`, `.cargo/`, `.config/`, `rust-toolchain.toml`, `rustfmt.toml`, `crates/README.md`, low-level crate manifests and READMEs, provisional foundation crates such as `andromeda-codec`, `andromeda-policy`, `andromeda-resource`, and crate topology tests. | Path-specific `git diff --check`; `cargo fmt --all --check`; workspace topology and forbidden-edge tests when manifests or crate boundaries change. |
| 02 - Specifications and implementation documentation | Review normative and planning documentation separately from runtime code. | `documentations/specs/`, `documentations/architecture/`, `documentations/implementation/`, legacy `docs/` migration paths, and documentation indexes. | Relative link check, ASCII check, trailing whitespace check, and documentation consistency review against Andromeda invariants. |
| 03 - Storage, WAL, transaction, and recovery | Review durable truth paths that can affect pages, WAL, recovery, backup, PITR, HA/DR, MVCC visibility, and visible commit. | `crates/andromeda-storage/`, `crates/andromeda-wal/`, `crates/andromeda-tx/`, relevant storage or WAL fuzz targets, and storage or WAL specs. | `cargo fmt --all --check`; owner crate checks; WAL, recovery, storage, and transaction tests; crash/recovery matrix; property or fuzz gates for byte formats. |
| 04 - Catalog, contracts, DefinitionBatch, and Procedure Store | Review typed contracts, catalog object model, DefinitionBatch, ProcedureContract, ContractHash, plan evidence, catalog publication, and Procedure Store boundaries. | `crates/andromeda-contract/`, `crates/andromeda-catalog/`, `crates/andromeda-procedure-store/`, relevant `andromeda-structured-object` paths, catalog and contract tests, and related specs. | ContractHash golden tests, compatibility tests, DefinitionBatch tests, catalog publication and WAL bridge tests, and crash gates when catalog publication changes. |
| 05 - SRPL language and compiler pipeline | Review SRPL syntax, parser, binder, cardinality, diagnostics, IR, lowering, optimizer safety, and compatibility facades. | `crates/andromeda-srpl/`, `crates/andromeda-srpl-ast/`, `crates/andromeda-srpl-cardinality/`, `crates/andromeda-srpl-diagnostics/`, `crates/andromeda-srpl-ir/`, `crates/andromeda-srpl-lexer/`, `crates/andromeda-srpl-parser/`, and SRPL fuzz targets. | SRPL owner tests, parser and AST direct tests, DefinitionBatch compatibility tests, optimizer safety tests, topology tests, and sustained fuzz evidence before parser promotion. |
| 06 - RPC, QUIC, security, IAM, audit, and surfaces | Review contract-first invocation boundaries, runtime-free protocol contracts, QUIC gateway behavior, admission ordering, IAM, audit, and surface separation. | `crates/andromeda-rpc-protocol/`, `crates/andromeda-quic/`, `crates/andromeda-security-contract/`, security portions of `andromeda-exec`, audit portions of `andromeda-observe`, CLI admin surface tests, protocol/security specs, and route/admission tests. | No-gRPC and no-runtime-JSON scans, protocol frame tests, QUIC route admission tests, fail-closed IAM tests, audit completion tests, wrong-surface tests, and pre-transaction rejection evidence. |
| 07 - Testing, fuzz, Miri, Loom, and release gate evidence | Review validation infrastructure, fuzz targets, seed corpora, test planning, release evidence templates, and tooling without claiming the gates passed. | `tests/`, `fuzz/`, `tools/testing/`, `.github/workflows/` validation jobs, `.github/scripts/` validation helpers, and `documentations/testing/`. | Fuzz target compile checks, targeted script tests, `git diff --check`, and retained evidence for any command that is presented as release proof. |
| 08 - Runbooks, governance, ADRs, risk, and release readiness | Review operational and governance records that constrain future release decisions. | `documentations/operations/runbooks/`, `documentations/operations/`, `documentations/governance/`, `docs/adr/`, release-readiness gates, risk register, C4/C5 control matrix, and supply-chain policy. | Link check, ADR/DEC consistency review, release-blocker review, and confirmation that future-dated or draft governance records are not used as current proof. |

### 3. Keep tests with their owners

Package crate-owned tests with the code or contract lot they validate whenever
possible. Use the testing lot for shared validation infrastructure, fuzz target
registration, root test labels, release evidence templates, and CI gate
plumbing.

Examples:

- A WAL codec property test travels with the storage/WAL lot.
- A parser owner test travels with the SRPL lot.
- A route admission test travels with the RPC/security lot.
- `tests/README.md`, fuzz registry changes, and release evidence templates
  travel with the testing/fuzz/Miri/Loom lot.

### 4. Separate release notes from release evidence

Release notes can summarize accepted packages only after evidence exists. A
release evidence record must include:

- Exact command.
- Date.
- Toolchain.
- Commit and branch.
- Pass, fail, skipped, or partial result.
- Retained artifact path.
- Residual risk.
- Owner or reviewer.

If any field is missing, the package can be reviewed but must not be described
as release-ready.

### 5. Final package review checklist

Before a package is handed to a PR author or release owner, verify:

1. The package has one named owner.
2. Every path is assigned to exactly one review lot.
3. No automatic staging or commit has occurred.
4. The branch remains treated as dirty until a clean candidate is proven.
5. All `MM` and `AD` paths in the package have explicit owner decisions.
6. The package has targeted whitespace, link, and ASCII checks where
   documentation is touched.
7. The package has owner tests or an explicit unrun-gate note.
8. C5 packages have crash/recovery or equivalent blocker notes before review.
9. Release wording says "candidate", "planning", "partial", or "blocked" unless
   retained release evidence proves acceptance.

## Validation

Validate this documentation and any future documentation-only packaging change
with targeted checks:

```powershell
git diff --check -- documentations/implementation/index.md documentations/implementation/roadmap-pr-packaging-2026-05-08.md
git diff --cached --check -- documentations/implementation/index.md documentations/implementation/roadmap-pr-packaging-2026-05-08.md
```

For this file, also run targeted documentation checks:

- Relative links resolve from `documentations/implementation/`.
- Files contain ASCII only.
- Files contain no trailing whitespace.

For future PR packages, run the package-specific gates from the lot table. A
package that changes Rust code, manifests, C4/C5 behavior, protocol/security
behavior, storage, WAL, catalog publication, or release tooling needs stronger
validation than this documentation-only guide.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| A package includes unrelated staged paths. | The current index contains other worker output. | Stop packaging. Use path-specific `git diff --cached --name-status` and require owner confirmation before any manual staging decision. |
| A reviewer asks for one PR containing the entire roadmap wave. | The lot boundaries are being treated as optional. | Split by the review lots in this guide and record dependencies between PRs instead of widening the package. |
| A package has many `MM` files. | Staged and working-tree content diverged during parallel work. | Inspect both sides path by path and have the owner produce the final working file before manual staging. |
| A package has `AD` files. | The index says the file is added, while the working tree says it is deleted. | Do not package until the owner decides whether the final state is present, deleted, or moved. |
| A release note says a gate passed, but no artifact exists. | Summary text was written before retained evidence. | Change the wording to `blocked`, `partial`, or `not yet recorded`, then add a release evidence record when the gate is run. |
| A C5 package cites specs, fuzz corpora, benchmark output, or audit output as truth. | Planning evidence is being confused with durable truth. | Require durable WAL, recovery, visibility, and owner test evidence; keep advisory evidence non-authoritative. |
| A formatter or link tool wants to touch files outside the package. | The tool is operating at repository scope. | Stop and rerun a path-targeted command, or hand off the unrelated paths to their owners. |

## References

- [Implementation Index](index.md)
- [Roadmap Execution Plan - 2026-05-08](roadmap-execution-plan-2026-05-08.md)
- [Worktree Packaging Plan - 2026-05-08](worktree-packaging-plan-2026-05-08.md)
- [Worker Wave Results - 2026-05-08](worker-wave-results-2026-05-08.md)
- [Step 11 Validation Matrix](../testing/step-11-validation-matrix.md)
- [Release Evidence Template](../testing/release-evidence-template.md)
- [Release Readiness Gates - 2026-05-08](../governance/release-readiness-gates-2026-05-08.md)
- [Risk Register - 2026-05-08](../governance/risk-register-2026-05-08.md)
