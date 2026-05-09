# Andromeda Tools

## Purpose

This directory contains repository-owned operational tools that are shared across local validation, CI, and release readiness workflows.

## Scope

Use this directory for scripts or small utilities that are not specific to a single crate and that support workspace validation, release evidence, dependency governance, or roadmap migration.

## Non-goals

Do not place runtime engine code, generated artifacts, benchmark output, fuzz corpora, or local machine state in this directory.

## Prerequisites

Tools in this directory must document their required runtime, input files, output files, and whether they are safe to run on a dirty worktree.

## Procedure

Add a dedicated subdirectory or script for each tool. Keep command-line behavior deterministic and avoid implicit network access unless the tool is explicitly for dependency or release validation.

## Validation

Repository-level tools should have either a focused test, a CI workflow invocation, or a documented manual validation command before they are treated as part of a release gate.

## Troubleshooting

If a tool writes files, document the exact output paths and whether those outputs are disposable evidence, committed fixtures, or release artifacts.

## Active Tools

### Testing

Read-only local validation and evidence helpers.

**Location**: `tools/testing/`
**Usage**: `python tools/testing/preflight.py`
**Purpose**: Local tool visibility, supply-chain preflight evidence, validation
manifest checks, and roadmap gate inventory.

## Retired Root Stubs

Root Rust tool placeholders are not retained without working behavior and
validation. Catalog diff ownership is `crates/andromeda-catalog-diff`. There is
no repository task-runner entry point until a real standalone runner is added.

## References

- `.github/workflows/00-ci.yml`
- `.github/workflows/release-gate-chain.yml`
- `docs/governance/supply-chain-policy.md`
- `crates/andromeda-catalog-diff/README.md`
- `.codex/scripts/validate_codex_tooling.py`
