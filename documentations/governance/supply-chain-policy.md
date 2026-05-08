# Supply Chain Policy

## Purpose

Define the release-gate policy for Rust dependency admission, duplicate dependency handling, advisory scanning, deny-list governance, and the current cargo-vet decision status for Andromeda.

## Scope

This policy applies to changes that add, remove, update, or reconfigure Cargo dependencies in the root workspace or any crate under `crates/**/Cargo.toml`.

It is binding for C5 durable-kernel crates, including `andromeda-wal`, `andromeda-storage`, `andromeda-tx`, and any future crate that owns WAL, recovery, storage format, MVCC, buffer pool, cold-store, backup, restore, or durability publication behavior.

## Non-goals

- This policy does not approve a new runtime surface.
- This policy does not replace explicit architecture decisions for critical dependencies.
- This policy does not make cargo-vet mandatory before the repository has an accepted cargo-vet configuration and decision record.
- This policy does not treat dependency tooling output as database truth.

## Prerequisites

Before admitting or updating a dependency, the owner must identify:

- The owning crate and engine boundary.
- Whether the dependency is runtime, development-only, build-only, or tooling-only.
- Whether the dependency can affect C5 durable behavior, recovery, security, RPC, catalog publication, or persistent/network formats.
- Whether the dependency introduces native code, unsafe abstractions, procedural macros, async runtime behavior, cryptography, compression, serialization, filesystem access, network access, or platform-specific behavior.

## Procedure

### C5 dependency admission

C5 durable-kernel dependencies require explicit admission before merge.

The admission record must state:

- Purpose: why the dependency is needed.
- Boundary: which crate owns it and which crates must not depend on it.
- Runtime class: runtime, development-only, build-only, or tooling-only.
- Format impact: whether it can influence WAL, page, segment, manifest, backup, restore, RPC, or catalog bytes.
- Recovery impact: whether it can affect replay, rollback, crash recovery, startup mode, or ForensicStart evidence.
- Disablement: how the dependency can be removed, isolated, or replaced if it regresses release safety.

C5 admission is rejected when a dependency:

- Introduces application-facing ad hoc SQL.
- Bypasses typed Procedure contracts.
- Serializes native Rust structs directly to disk or network.
- Makes RAM, temp storage, GPU output, or benchmark output authoritative.
- Adds GPU work to commit, WAL, rollback, recovery, MVCC short-visibility, catalog publication, or security-critical paths.
- Adds unbounded runtime behavior to recovery, WAL, RPC, security, or catalog publication paths.

### Advisory and deny gates

Release gates must run:

```bash
cargo audit --deny warnings
cargo deny check --all-features
```

The dedicated supply-chain workflow and the release gate chain both trigger on:

- `Cargo.toml`
- `Cargo.lock`
- `crates/**/Cargo.toml`
- `deny.toml`
- the owning workflow files

The `cargo-audit` gate blocks release on unignored RustSec advisories, yanked dependency policy violations, and warning-level advisory findings. The `cargo-deny` gate enforces allowed licenses, registry/source policy, wildcard dependency denial, and the duplicate dependency policy in `deny.toml`.

### Duplicate dependency policy

Duplicate versions are currently classified as `warn` in `deny.toml`.

That status is intentional. Duplicate versions are not automatically release-blocking because the repository still contains transitional dependency ownership across durable-kernel, protocol, and tooling crates. A duplicate becomes release-blocking when any of the following are true:

- It appears in a C5 durable-kernel runtime dependency path.
- It changes persistent or network byte interpretation.
- It changes cryptographic, checksum, filesystem, recovery, backup, restore, or startup behavior.
- It duplicates an async runtime, TLS, QUIC, serialization, compression, or platform abstraction dependency in a runtime path without owner approval.
- It creates measurable binary-size, compile-time, or performance regression in a release-critical path.

Allowed duplicate warnings must have an owner and a removal or retention rationale. New C5 duplicates should be avoided unless the owner documents why unification is riskier than the duplicate.

### cargo-vet decision status

`cargo-vet` is planned, not mandatory, as of this policy version.

The repository does not currently contain an accepted cargo-vet configuration such as `supply-chain/config.toml`, `cargo-vet.toml`, or `.cargo-vet`. Therefore, workflows may record cargo-vet status as preview evidence but must not fail a release solely because cargo-vet is unavailable.

Promoting cargo-vet to a blocking gate requires:

- A decision record that defines the vetting authority, import policy, exemption policy, and review cadence.
- A committed cargo-vet configuration.
- A documented bootstrap procedure for first-party audits and third-party imports.
- A workflow command that is known to exist in the repository and can run deterministically in CI.

## Validation

Required CI evidence:

```bash
cargo audit --deny warnings
cargo deny check --all-features
```

Release readiness also requires the mission-critical gates that already have repository commands:

```bash
cargo test -p andromeda-storage --test property_recovery_replay --locked -- --nocapture
cargo test -p andromeda-storage --test recovery_completeness_contract --locked -- --nocapture
cargo test -p andromeda-storage --test backup_physical_plan_contract --locked -- --nocapture
cargo test -p andromeda-storage --test backup_execution_plan --locked -- --nocapture
cargo test -p andromeda-storage --test restore_contract --locked -- --nocapture
cargo test -p andromeda-storage --locked forensic_start -- --nocapture
```

Full PITR, cluster backup/restore, and incident-grade ForensicStart drills remain planned until the repository contains stable commands for those drills.

## Troubleshooting

If `cargo audit` fails, classify the advisory as blocker, accepted risk, false positive, or out-of-scope development-only exposure. Do not ignore an advisory without owner rationale.

If `cargo deny` fails, update the dependency, tighten the license/source entry, or add an explicit policy exception. Do not weaken deny policy to merge a release-critical dependency without release-governance approval.

If duplicate dependency warnings increase, inspect the reverse dependency graph and determine whether the duplicate is runtime, development-only, or tooling-only. C5 runtime duplicates require owner review.

If cargo-vet is requested before configuration exists, record the request as planned governance work and keep CI non-blocking until the cargo-vet decision record and configuration are accepted.

## References

- `deny.toml`
- `.github/workflows/05-supply-chain.yml`
- `.github/workflows/release-gate-chain.yml`
- `documentations/governance/decisions/DEC-026-release-gates-and-deferral-policy.md`
- `documentations/governance/decisions/DEC-035-release-gate-chain.md`
