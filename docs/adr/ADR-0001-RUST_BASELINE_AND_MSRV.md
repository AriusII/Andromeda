# ADR-0001-RUST BASELINE AND MSRV — Rust baseline and MSRV

> **Status:** Accepted for V0 documentation baseline  
> **Scope:** Andromeda architecture and implementation governance  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, resolver 3, 89 crates

## Context

Andromeda targets an enterprise-grade relational transactional engine with a strict Procedure surface, typed contracts, WAL-first durability, recovery evidence, and bounded adaptive internals.

## Decision

Use Rust 1.95.0, Rust 2024 Edition, workspace resolver 3, and the 89-crate root workspace as the documentation and engineering baseline.

The root `Cargo.toml` is the source of truth for P00 workspace membership. Historical references to 88 crates are stale unless explicitly preserved as historical context.

## Required proof

Baseline claims must be supported by retained evidence that names the source artifact and the validation command or review record. For P00, the minimum proof is:

| Claim | Source of truth | Evidence |
|---|---|---|
| Rust version | Root `Cargo.toml` and package metadata | `rust-version = "1.95.0"` or `cargo metadata` package output. |
| Edition | Root `Cargo.toml` and package metadata | Edition `2024` across workspace packages. |
| Resolver | Root `Cargo.toml` | `resolver = "3"`. |
| Crate count | Root `Cargo.toml` workspace members | 89 workspace members in `cargo metadata --format-version 1 --no-deps`. |

These checks prove repository baseline alignment only. They do not prove production readiness, release readiness, durability, security, recovery, HA/DR, or operational safety.

## Rationale

This decision reduces ambiguity and prevents implementation drift across architecture, code, tests, and operations.

## Consequences

### Positive

- The implementation boundary is explicit.
- Reviewers can reject incompatible shortcuts.
- Tests can be mapped to the decision.
- Operational behavior is easier to explain after an incident.

### Negative

- Some implementation shortcuts are intentionally unavailable.
- Additional tests and documentation are required.
- Experimental work must be isolated before entering critical paths.

## Validation

This ADR is validated by:

- root `Cargo.toml` and `cargo metadata --format-version 1 --no-deps` for P00 baseline evidence;
- a matching specification when the decision affects a technical structure;
- a test plan when the decision affects runtime behavior;
- a runbook when the decision affects operations;
- trace or audit evidence when the decision affects security, durability, or recovery.

## Rejection criteria

Reject implementation work that contradicts this decision without a superseding ADR.

Reject documentation that describes the workspace as 88 crates, Edition 2021, resolver 1/2, or any Rust baseline other than 1.95.0 unless the text is explicitly marked as historical and points readers back to the current status document.
