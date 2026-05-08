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

## Tools

### xtask

Task orchestrator for common development workflows. Entry point for CI/build/test automation.

**Location**: `tools/xtask/`  
**Usage**: `cargo xtask [command]`  
**Purpose**: Unified task runner, build automation, test orchestration

### WAL Dump

Diagnostic tool for inspecting Write-Ahead Log files.

**Location**: `tools/wal-dump/`  
**Usage**: `cargo run --bin wal-dump -- <wal_file>`  
**Purpose**: WAL forensics, record inspection, LSN analysis

### Page Dump

Diagnostic tool for inspecting storage pages.

**Location**: `tools/page-dump/`  
**Usage**: `cargo run --bin page-dump -- <page_file>`  
**Purpose**: Page forensics, header inspection, layout validation

### Catalog Diff

Tool for comparing catalog snapshots and versions.

**Location**: `tools/catalog-diff/`  
**Usage**: `cargo run --bin catalog-diff -- <snapshot1> <snapshot2>`  
**Purpose**: Schema evolution tracking, breaking change detection

### Crash Runner

Tool for orchestrating crash injection and recovery testing.

**Location**: `tools/crash-runner/`  
**Usage**: `cargo run --bin crash-runner -- --scenario <name>`  
**Purpose**: Crash scenario execution, recovery validation

### Schema Gen

Tool for generating schema-related code from specifications.

**Location**: `tools/schema-gen/`  
**Usage**: `cargo run --bin schema-gen -- --schema <spec>`  
**Purpose**: Code generation, schema validation

## References

- `.github/workflows/00-ci.yml`
- `.github/workflows/release-gate-chain.yml`
- `.codex/scripts/validate_codex_tooling.py`
