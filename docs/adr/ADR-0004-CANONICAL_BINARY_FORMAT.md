# ADR-0004-CANONICAL BINARY FORMAT — Canonical binary format

> **Status:** Accepted for V0 documentation baseline  
> **Scope:** Andromeda architecture and implementation governance  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, resolver 3, 89 crates

## Context

Andromeda targets an enterprise-grade relational transactional engine with a strict Procedure surface, typed contracts, WAL-first durability, recovery evidence, and bounded adaptive internals.

## Decision

Use explicit little-endian canonical codecs and never persist Rust native layouts.

Persisted files, WAL frames, page formats, manifests, segment metadata, backup/restore artifacts, and protocol frames must have explicit byte contracts. Native Rust memory layout, implicit `repr(Rust)` layout, pointer layout, process-local enum discriminants, and opaque derived binary serialization are not canonical storage or wire formats.

Custom Protobuf may define typed RPC payload contracts where the owning protocol spec permits it. Protobuf usage does not create a gRPC application surface and does not replace explicit framing, bounds checks, version checks, or ResultStream ordering rules.

Diagnostic JSON, benchmark output, traces, and CLI output may support humans or tests, but they are not native protocol, storage truth, recovery truth, or catalog truth.

## Format proof rule

Any canonical binary format change must retain evidence for the affected path:

| Path | Required evidence |
|---|---|
| WAL, page, heap, B-Tree, manifest, segment, backup, or restore bytes | Owner spec or ADR, byte-for-byte roundtrip or golden tests, corruption rejection, and crash/recovery evidence where durable state is touched. |
| RPC frames or ResultStream metadata/payload order | Protocol spec or ADR, bounded decode tests, frame ordering tests, and rejection tests for malformed payloads. |
| Contract hashes or descriptor bytes | Canonical field order, version binding, deterministic digest tests, and compatibility tests. |

A compile check or successful demo is not sufficient evidence for a C4/C5 binary format.

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

- owning format specifications, golden/roundtrip tests, malformed-input rejection tests, and recovery evidence where durable state is affected;
- a matching specification when the decision affects a technical structure;
- a test plan when the decision affects runtime behavior;
- a runbook when the decision affects operations;
- trace or audit evidence when the decision affects security, durability, or recovery.

## Rejection criteria

Reject implementation work that contradicts this decision without a superseding ADR.

Reject persisted or native protocol formats that depend on Rust native layout, unbounded decoding, diagnostic JSON, or framework defaults not fixed by an Andromeda-owned byte contract.
