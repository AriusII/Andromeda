# Dependency Policy

## Purpose

This document defines dependency direction, temporary facade policy, and review
gates for workspace topology changes. It complements ADR ring policy and the
domain specs under `docs/specs/`.

## Ring direction

| Source | May depend on | Must not depend on |
| --- | --- | --- |
| R0 foundation | R0 only, with documented leaf or facade exceptions. | Engine runtime, catalog store, execution, storage, WAL, transaction, transport runtime, benchmarks, analytics, GPU runtime, SQL, gRPC, runtime JSON default, native-layout persistence. |
| R1 contracts, protocol contracts, SRPL model | R0 and lower-risk R1 contract-safe crates. | Runtime stores, Quinn/TLS runtime, async runtime, execution, storage, WAL, recovery, benchmarks, analytics, GPU, SQL, gRPC, runtime JSON default, native-layout persistence. |
| R2 durable kernel | R0, runtime-free R1 contracts, WAL-safe contracts, typed observability evidence when needed. | SRPL parser/model, catalog store implementations except explicitly owned catalog durability paths, execution, protocol runtime, QUIC runtime, benchmarks, analytics, GPU, SQL, gRPC, runtime JSON default, native-layout persistence. |
| R3 execution | R0, R1 contracts, R2 durable APIs, typed observability. | Becoming a dependency of R0/R1 model crates, WAL, storage, transaction owners, or protocol-contract crates. |
| R4 transport runtime | R0, runtime-free protocol and security contracts, typed observability. | Storage truth, WAL authority, Procedure semantics, catalog publication, IAM policy ownership. |
| R5 tools and benchmarks | Lower rings for operator commands, tests, benchmarks, and diagnostics. | Production crates depending on R5 tooling or benchmark output as authority. |

Dependencies should point from higher-level orchestration toward lower-level
contracts or owners. Reverse edges require a documented exception, bounded
scope, and an exit condition.

## Temporary facades

| Facade | Canonical owner | Allowed role | Exit gate |
| --- | --- | --- | --- |
| Foundation compatibility paths | Foundation owner crates and future principal/security owner. | Preserve historical imports while callers migrate. | Direct owner imports, principal owner decision, facade compatibility, topology allowlist. |
| Catalog contract paths | Contract owner. | Preserve contract imports while catalog keeps store and publication ownership. | Caller migration, contract facade tests, catalog keeps only catalog runtime ownership. |
| Protocol StructuredObject paths | StructuredObject owner. | Preserve payload imports while protocol crates own schema and projection. | Caller migration, StructuredObject owner tests, protocol compatibility tests. |
| QUIC frame and stream paths | RPC protocol owner. | Preserve runtime-free protocol imports through transport crate. | Caller migration, frame and stream owner tests, QUIC compatibility tests. |
| SRPL root and compiler paths | Extracted SRPL model crates plus bridge owners. | Preserve historical compiler and bridge imports. | Model caller migration, dedicated bridge owner, parser and model topology gates. |
| Storage WAL and FileWal paths | WAL owner. | Preserve storage-era WAL imports. | Direct WAL caller migration, WAL owner tests, storage compatibility and recovery integration tests. |
| Transaction root or MVCC paths | Transaction and concurrency owners. | Preserve imports during future transaction splits. | Future owner crates, transaction compatibility tests, durable commit and replay gates. |

A facade may reexport or adapt. It must not define new canonical behavior. If
new behavior appears in a facade, move it to the owner or record a decision
that changes ownership.

## Review gates

| Gate | Applies to | Required evidence |
| --- | --- | --- |
| Ownership statement | New crate, split, or moved behavior. | Canonical owner, facade if any, ring, dependency direction, and exit condition. |
| Topology | Manifest or dependency changes. | `cargo metadata --no-deps --format-version 1`, path-specific dependency review, and owner tests for affected rings. |
| Runtime-free allowlist | R0/R1 contracts, protocol, security, SRPL model. | No runtime stores, Quinn/TLS runtime, async runtime, storage, WAL, recovery, benchmarks, GPU, SQL, gRPC, runtime JSON default, or native-layout persistence. |
| Facade compatibility | Moved public import. | Old-import tests plus direct owner tests before facade removal. |
| Persistent bytes | WAL, page, heap, B-Tree, manifest, backup, recovery, catalog WAL. | Explicit codec, format identity, endian policy, version fields, length bounds, checksum or digest, golden vectors, malformed-input rejection. |
| Network bytes | RPC frames, protobuf payloads, envelopes, completions. | Explicit frame codec or schema projection, no gRPC, no runtime JSON default, malformed frame tests, metadata-before-payload tests. |
| C5 durable behavior | WAL, storage, transaction, recovery, catalog publication, backup, restore, HA/DR. | Owner tests, integration tests, property or fuzz tests, crash/recovery matrix, replay rejection, durable visibility evidence. |
| Security-critical behavior | Admission, IAM, mTLS, permissions, surface routing, audit. | Permission denial, wrong surface, disabled principal, no transaction on rejection, audit evidence, threat model. |
| Advisory behavior | Statistics, optimizer, plan cache, Maps, scenario evidence, benchmarks. | Bounded candidates, version binding, DecisionTrace, disablement, stale evidence rejection, advisory-only evidence proof. |
| GPU or SIMD | Optional hardware acceleration. | CPU fallback, disablement, trace fields, no critical-path import scan, and proof that failure does not alter contractual or recovered results. |

## Forbidden dependency outcomes

- Foundation or model crates depending on storage, WAL, recovery, transport
  runtime, benchmarks, GPU runtime, SQL, gRPC, or runtime JSON defaults.
- Durable kernel crates depending on advisory analytics or transport runtime
  to decide durable truth.
- Production crates depending on benchmark or scenario evidence as authority.
- Transport runtime owning Procedure semantics, catalog publication, IAM policy,
  WAL authority, or storage truth.
- Audit, traces, or observability becoming database truth.
- GPU or SIMD output on commit, WAL, rollback, recovery, security, catalog
  publication, or MVCC visibility paths.

## Validation posture by change type

| Change type | Minimum validation |
| --- | --- |
| Documentation-only architecture or spec consolidation | Link checks and process-jargon scan for owned docs. |
| Public facade migration | Old import tests, new owner tests, and topology scan. |
| Runtime-free contract change | Unit tests, compatibility tests, and dependency allowlist. |
| Persistent byte-format change | Golden vectors, malformed-input rejection, checksum or digest tests, and recovery compatibility tests. |
| C5 behavior change | Owner tests plus crash/recovery, replay, visibility, rollback, or restore evidence. |
| Security or surface change | Denial tests, wrong-surface tests, disabled-principal tests, no-transaction-on-rejection tests, audit evidence. |
| Advisory optimizer or hardware change | Disablement, bounded candidates, stale-evidence rejection, traceability, CPU fallback when relevant. |
