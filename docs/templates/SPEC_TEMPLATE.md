# Specification: Name v0

> **Status:** Draft or Normative V0 specification
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers
> **Baseline:** Rust 1.95.0, Rust 2024 Edition

## Purpose

## Scope

## Source grounding

List the doctrine, roadmap phase, ADR, previous spec, or retained evidence artifact that this
spec depends on. Do not introduce normative behavior from memory or from an implementation detail
without a source reference.

## Non-goals

## Criticality and evidence classification

State the highest `C0` through `C5` criticality touched by this spec and name the affected
doctrine invariant when applicable.

For every C4/C5 behavior, list the retained evidence required before implementation or release
claims are accepted. Generic compile success, demos, scaffolds, benchmarks, simulation-only drills,
or trace existence are not sufficient evidence by themselves.

## Data structures

Each persisted, network-visible, security-critical, or recovery-critical structure must be named and owned.

## Owner crates

List the implementation owner crate or crates. If the spec is documentation-only, name the future owner and the phase that must create it.

## Invariants

## Serialization

State whether the structure is persisted, network-visible, runtime-only, or semantic IR.

Persisted and network-visible formats must use explicit canonical encoding. Native Rust layout is not a valid format.

## State transitions

## Error model

Errors must be typed. String-only critical errors are not sufficient evidence.

## Security model

## Observability

## Recovery behavior

If this spec touches durable state, WAL, MVCC visibility, catalog truth, backup, restore, HA/DR,
or visible commit behavior, define the crash/recovery evidence required for closure. Otherwise,
state explicitly that no durable recovery behavior is touched.

## Compatibility

## Tests

List success, rejection, property, golden, fuzz, crash/recovery, or fail-closed tests as applicable.

## Acceptance evidence

List the exact evidence required before implementation can rely on this spec: owner crate, targeted tests, rejection tests, trace/audit evidence, golden vectors, fuzz, crash/recovery, or fail-closed behavior as applicable. This section prepares, but does not replace, the required `Owner`, `Evidence`, and `Reject` lines in the acceptance summary.

## Rejection criteria

Use explicit `Reject ...` entries. Do not leave rejection behavior implicit.

At minimum, reject readiness or implementation claims when required source grounding, criticality
classification, retained evidence, crash/recovery evidence, or fail-closed evidence is missing.

## Acceptance summary

State the exact proof needed before this spec can unblock implementation. The summary must include
literal `Owner`, `Evidence`, and `Reject` entries.

- Owner: name the owner crate or future owner crate and responsible phase.
- Evidence: name the retained source artifact plus validation command, review record, golden vector,
  crash/recovery proof, or fail-closed proof required by this spec.
- Reject: name the readiness, implementation, or release claim that must be rejected when the owner
  or evidence is missing.

The acceptance summary must tie the changed path to its criticality, source artifact, validation
command or review record, and crash/recovery or fail-closed evidence when the spec touches durable
state, external surfaces, security, admission, audit, backup, restore, HA/DR, WAL, MVCC, or catalog
truth.
