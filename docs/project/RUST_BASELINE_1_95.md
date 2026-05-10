# Rust baseline 1.95.0

> **Status:** Normative language baseline  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the Rust version baseline.
- Define edition, workspace, and tooling expectations.
- Define the workspace count source of truth.
- Establish rules for nightly usage and unsafe code visibility.

## Baseline decision

Andromeda documentation assumes:

```text
Rust version: Rust 1.95.0
Edition: Rust 2024
Workspace resolver: 3
Workspace crates: 89 declared members
Primary targets: x86_64 and aarch64
Production toolchain: stable Rust
Nightly use: tooling gates only unless an ADR approves otherwise
```

## Rationale

Rust 1.95.0 is used as the explicit 2026 baseline for this documentation package. The goal is not to chase language novelty. The goal is to standardize on a current stable toolchain that supports a serious systems-engineering process.

## Workspace rules

The repository should use a Cargo workspace with centralized policy:

```toml
[workspace]
resolver = "3"

[workspace.package]
edition = "2024"
rust-version = "1.95.0"

[workspace.lints.rust]
unsafe_op_in_unsafe_fn = "warn"
unreachable_pub = "warn"
missing_docs = "warn"
```

The root `Cargo.toml` is the P00 source of truth for workspace membership.
The current baseline is 89 declared crates. Documentation, roadmap status, and
topology tests must be updated when `workspace.members` changes.

## P00 validation rule

Repository-state claims require executable evidence. For the Rust baseline and
workspace topology, the common proof is:

```powershell
cargo metadata --no-deps --locked --format-version 1
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

The first command proves Cargo can resolve the declared workspace. The second
command proves P00 topology rules, including the 89-crate count and the match
between root `Cargo.toml` members and physical crate manifests under `crates/`.

## Nightly policy

Nightly is allowed for:

- Miri;
- fuzzing;
- sanitizer jobs;
- experimental CPU/GPU research;
- compiler diagnostics exploration.

Nightly-only language features are not allowed in C5 production paths unless an ADR defines the scope, fallback, and rollback plan.

## Unsafe policy baseline

Rust 1.95.0 does not remove the need for unsafe governance.

Every unsafe block must have:

```text
local SAFETY explanation
module-level invariant
bounded API surface
tests or fuzzing when applicable
review evidence
```

## Production rule

> [!IMPORTANT]
> A production Andromeda binary must not rely on hidden nightly behavior, undocumented target assumptions, or native Rust struct layouts for persisted data.

Criticality labels, scaffold crates, demos, benchmarks, traces, and roadmap text
are not production-readiness evidence. A production-readiness claim requires the
phase-specific retained evidence named by the roadmap and acceptance gates.
