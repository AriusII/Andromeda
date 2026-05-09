# Architecture Overview

## Purpose

This document describes the durable architecture rules that govern the
Andromeda workspace. Domain-level byte formats and subsystem contracts live in
`docs/specs/`; this file explains how the major planes fit together.

## System planes

| Plane | Current responsibility | Boundary |
| --- | --- | --- |
| Foundation | Errors, digests, semantic identifiers, time, hardware descriptors, explicit codec helpers. | Must not depend on engine runtime, storage truth, transport runtime, benchmarks, GPU runtime, SQL, gRPC, runtime JSON defaults, or native-layout persistence. |
| Contracts and models | Procedure contracts, StructuredObject descriptors, protocol contracts, security vocabulary, SRPL syntax and IR. | Runtime-free unless a documented bridge owns integration. |
| Durable kernel | WAL, storage, pages, segments, manifests, recovery, backup, restore, HA/DR, transactions, MVCC, locks, savepoints. | Durable truth and visibility require owner tests, explicit codecs, and crash/recovery evidence. |
| Catalog and Procedure publication | Catalog store, DefinitionBatch, Procedure Store, catalog WAL records, publication, catalog recovery, statistics metadata. | Visible catalog state requires validation, compatibility, dependency checks, and durable WAL. |
| Execution | Admission, dispatch, transaction binding, Procedure runtime, result streams, retry, execution traces, SRPL adapters. | May create transactions only after contract and security admission succeed. |
| Transport and security | QUIC transport, runtime-free RPC protocol, IAM and principal paths, admission, audit, surface routing. | Must reject malformed or unauthorized requests before Procedure dispatch and before transaction creation. |
| Advisory and analytics | Statistics, optimizer, plan cache, Maps, scenario evidence, benchmarks, hardware policy. | Advisory, bounded, versioned, explainable, disableable, and never durable truth by itself. |
| Tools and operations | CLI, demos, test support, benchmark harnesses, operator commands. | Tooling must not become a production dependency or hide readiness gaps. |

## Criticality scale

| Level | Name | Design rule | Typical validation |
| --- | --- | --- | --- |
| C0 | Isolated experimental | Never enters a critical path. | Isolation proof, feature gating, removal path. |
| C1 | Opportunistic | Disableable without consistency impact. | Unit tests and disablement proof. |
| C2 | Important | Measured, budgeted, and observable. | Unit and integration tests, metrics, and budget checks. |
| C3 | Critical | Versioned, audited, and policy-bound. | Contract tests, compatibility tests, audit checks, and policy gates. |
| C4 | Mission-critical | Tested in recovery or crash scenarios when it can affect state or admission. | Property tests, fuzz tests, threat-model review, crash/recovery matrix, targeted integration gates. |
| C5 | Non-negotiable | Guaranteed by design and not bypassable. | C4 gates plus durable truth, visible-commit, replay, rollback, and release evidence. |

WAL, storage truth, recovery, visible transaction state, catalog publication,
backup, restore, and HA/DR promotion are C5 when behavior changes can affect
durability or visibility.

## Durable truth

The durable truth boundary is the latest accepted snapshot or manifest plus the
validated durable WAL required from that point. Audit, traces, benchmark output,
statistics, GPU output, and plan choices are evidence. They do not become truth
unless the owning spec routes them through validation and durable publication.

Commit visibility requires durable commit WAL evidence. Rollback completion
requires durable rollback WAL evidence. Catalog publication requires durable
catalog WAL evidence. Dirty page flush requires durable WAL coverage for the
page's latest dirty LSN.

## Surface separation

Application, Administration, Monitoring, BackupAgent, Cluster or HA/DR, and
Forensic surfaces are separate security and routing domains. Application may
invoke only typed cataloged Procedures and allowed contract metadata reads.
Privileged operations must use their owning surface and admission policy.

## Format policy

Persistent and network formats must use explicit codecs with version fields,
byte order markers, length bounds, checksum or digest evidence, and malformed
input rejection. Rust struct memory layout, unbounded JSON, display text, and
debug projections are not durable or network contracts.

## Architecture anti-patterns

- Treating a facade reexport as canonical ownership.
- Letting model or contract crates depend on runtime stores or transport
  runtimes.
- Allowing advisory output to authorize critical transitions.
- Publishing catalog, manifest, or transaction state before durable WAL.
- Replaying audit, traces, or decision evidence as execution.
- Hiding surface routing or required permissions in client-provided text.
- Allowing broad crates to gain unrelated responsibilities without ownership
  and validation updates.
