# Dependency Edge Matrix - 2026-05-08

## Purpose

This ledger records the workspace dependency edges observed for Step 0 governance on 2026-05-08. It gives later restructuring packets a stable review surface before they move code, remove facades, or claim that a crate boundary is accepted.

## Scope

This document applies to the current Andromeda Rust workspace under the root `Cargo.toml`. `cargo metadata --no-deps --format-version 1` reports 32 workspace packages in the current worktree, and the root `Cargo.toml` explicit `members` list contains the same 32 paths in the final Step 0 snapshot.

The ledger covers workspace edges declared in package manifests. It does not enumerate every external dependency, feature flag, transitive dependency, or source-level import.

## Non-goals

- Do not use this document as release-completeness evidence.
- Do not treat an observed edge as accepted only because it appears in this matrix.
- Do not use this document to authorize new C4 or C5 runtime coupling.
- Do not use this document to bypass ADR-0011 or the topology tests.
- Do not infer that compile, clippy, fuzz, crash/recovery, Miri, audit, or release gates passed.

## Prerequisites

Before using this matrix for a packet, read:

- `AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `documentations/architecture/WORKSPACE_RESTRUCTURE_BASELINE_2026.md`

Re-run `cargo metadata --no-deps --format-version 1` after any manifest change. If the worktree has staged and unstaged divergence, use the working tree only as the local evidence snapshot and avoid release claims.

## Procedure

1. Use the root `Cargo.toml` as the list of workspace members.
2. Use `cargo metadata --no-deps --format-version 1` to extract direct package dependencies.
3. Separate runtime workspace edges from dev-only workspace edges.
4. Classify each edge against ADR-0011 and the topology guard in `crates/andromeda-cli/tests/workspace_dependency_topology.rs`.
5. Treat temporary exceptions as active debt with exit criteria, not as permanent architecture.
6. Re-run the topology gate before accepting any package that changes manifests.

## Dependency Matrix

| Source crate | Runtime workspace dependencies | Dev-only workspace dependencies | Step 0 governance note |
| --- | --- | --- | --- |
| `andromeda-bench` | `andromeda-error`, `andromeda-hardware`, `andromeda-observe`, `andromeda-srpl`, `andromeda-storage`, `andromeda-time`, `andromeda-types` | None observed | R5 evidence and benchmark crate. It may exercise engine crates, but its output remains advisory and must not become C5 truth. |
| `andromeda-catalog` | `andromeda-contract`, `andromeda-digest`, `andromeda-error`, `andromeda-observe`, `andromeda-proto`, `andromeda-time`, `andromeda-types` | `andromeda-storage` | Broad catalog owner with a named catalog-to-proto watch edge and a storage dev harness. Catalog publication remains C5-sensitive. |
| `andromeda-cli` | `andromeda-bench`, `andromeda-catalog`, `andromeda-core`, `andromeda-exec`, `andromeda-observe`, `andromeda-quic`, `andromeda-srpl`, `andromeda-storage` | None observed | R5 operator and developer interface. CLI commands must not bypass typed Procedure contracts or surface Admin and HA/DR behavior through the Application Surface. |
| `andromeda-codec` | None observed | None observed | Dependency-free codec scaffold. It must stay explicit-format oriented and must not become native-layout serialization. |
| `andromeda-contract` | `andromeda-digest`, `andromeda-error`, `andromeda-types` | None observed | Contract-safe owner for Procedure contracts, qualified names, catalog object descriptors, and structural dependency edges. |
| `andromeda-core` | `andromeda-digest`, `andromeda-error`, `andromeda-hardware`, `andromeda-security-contract`, `andromeda-time`, `andromeda-types` | None observed | Temporary R0 compatibility facade. The `andromeda-security-contract` edge is a documented exception for principal vocabulary, not IAM runtime ownership. |
| `andromeda-digest` | None observed | None observed | R0 foundation leaf crate. |
| `andromeda-error` | None observed | None observed | R0 foundation leaf crate. |
| `andromeda-exec` | `andromeda-catalog`, `andromeda-core`, `andromeda-observe`, `andromeda-proto`, `andromeda-quic`, `andromeda-srpl`, `andromeda-storage`, `andromeda-tx` | None observed | R3 execution orchestrator. The direct `andromeda-quic` edge is a temporary bridge debt recorded by ADR-0011. |
| `andromeda-hardware` | `andromeda-error` | None observed | R0 hardware descriptor and critical-path exclusion vocabulary. It must not introduce GPU execution ownership. |
| `andromeda-maps` | None observed | None observed | Map descriptor scaffold. Maps remain advisory or analytical until durable source, refresh, summarizability, and validation rules are proven. |
| `andromeda-observe` | `andromeda-core`, `andromeda-digest`, `andromeda-error`, `andromeda-hardware`, `andromeda-types` | `andromeda-storage` | Observability and audit evidence owner. The storage dev edge is a test harness edge and must not turn audit evidence into database truth. |
| `andromeda-policy` | None observed | None observed | Policy and admission scaffold. It must not become an ungoverned IAM runtime or bypass `andromeda-security-contract`. |
| `andromeda-procedure-store` | None observed | None observed | Procedure Store scaffold. It must remain versioned evidence until durable owner and catalog integration gates are proven. |
| `andromeda-proto` | `andromeda-digest`, `andromeda-error`, `andromeda-structured-object`, `andromeda-types` | None observed | Typed payload and protocol schema crate. It keeps StructuredObject compatibility reexports while avoiding gRPC runtime ownership. |
| `andromeda-quic` | `andromeda-core`, `andromeda-observe`, `andromeda-proto`, `andromeda-rpc-protocol`, `andromeda-security-contract` | None observed | R4 transport runtime crate. It maps concrete transport behavior to typed protocol boundaries and temporarily reexports runtime-free RPC protocol contracts. |
| `andromeda-resource` | None observed | None observed | Resource limit scaffold. It must stay bounded and observable, and it escalates when used by admission or critical path throttling. |
| `andromeda-rpc-protocol` | `andromeda-core` | None observed | Runtime-free frame and stream contract owner. The `andromeda-core` edge keeps current foundation imports behind the facade. |
| `andromeda-security-contract` | None observed | None observed | Runtime-free security vocabulary crate in the current snapshot. It is not an IAM runtime, policy store, revocation store, audit ledger, or TLS runtime owner. |
| `andromeda-srpl` | `andromeda-catalog`, `andromeda-digest`, `andromeda-error`, `andromeda-srpl-ast`, `andromeda-srpl-cardinality`, `andromeda-srpl-diagnostics`, `andromeda-srpl-ir`, `andromeda-srpl-lexer`, `andromeda-srpl-parser`, `andromeda-types` | None observed | SRPL compatibility facade. The catalog edge is an active bridge exception for DefinitionBatch and resolver work; the lexer edge points to the extracted tokenizer owner. |
| `andromeda-srpl-ast` | `andromeda-contract`, `andromeda-srpl-cardinality`, `andromeda-srpl-diagnostics`, `andromeda-types` | None observed | Extracted language-model crate. It must remain catalog-store-free. |
| `andromeda-srpl-cardinality` | `andromeda-contract` | None observed | Extracted cardinality crate. It must remain runtime-free. |
| `andromeda-srpl-diagnostics` | `andromeda-error` | None observed | Extracted diagnostics crate. It must remain parser/runtime-store independent. |
| `andromeda-srpl-ir` | `andromeda-contract`, `andromeda-error`, `andromeda-srpl-cardinality`, `andromeda-types` | None observed | Extracted semantic IR crate. It must remain free of catalog store, execution, storage, transport, and benchmark crates. |
| `andromeda-srpl-lexer` | `andromeda-srpl-diagnostics` | None observed | Extracted tokenizer crate observed through `cargo metadata`. It must remain parser-only and catalog-store-free. |
| `andromeda-srpl-parser` | `andromeda-contract`, `andromeda-srpl-ast`, `andromeda-srpl-cardinality`, `andromeda-srpl-diagnostics`, `andromeda-srpl-lexer`, `andromeda-types` | None observed | Extracted parser crate. It must remain catalog-store-free and import tokenization from the lexer owner. |
| `andromeda-storage` | `andromeda-core`, `andromeda-observe`, `andromeda-wal` | None observed | C5 durable-kernel owner and compatibility facade. `andromeda-core` is temporary debt; `andromeda-wal` is the canonical lower-level WAL owner edge. |
| `andromeda-structured-object` | `andromeda-digest`, `andromeda-error`, `andromeda-types` | None observed | Contract-safe StructuredObject owner. |
| `andromeda-time` | `andromeda-error` | None observed | R0 foundation crate. |
| `andromeda-tx` | `andromeda-core`, `andromeda-observe` | None observed | C5 transaction-kernel crate. `andromeda-core` is temporary facade debt; observability remains evidence, not truth. |
| `andromeda-types` | `andromeda-error` | None observed | R0 identifiers and scalar type descriptors. |
| `andromeda-wal` | `andromeda-core` | None observed | C5 WAL owner for pure WAL primitives and physical FileWal byte contracts. `andromeda-core` is temporary foundation facade debt. |

## Watch Edges

| Edge | Current reason | Required handling |
| --- | --- | --- |
| `andromeda-catalog` -> `andromeda-proto` | Catalog descriptors still publish protocol-facing schema manifests. | Keep one-way until catalog/proto contracts split and the topology guard is updated. |
| `andromeda-exec` -> `andromeda-quic` | Existing executor bridge predates the ring policy. | Move the bridge to a transport adapter or abstract protocol boundary in a later packet. |
| `andromeda-storage` -> `andromeda-core` | C5 crate still imports the wide foundation facade. | Replace with extracted durable-storage foundation imports before widening the exception list. |
| `andromeda-tx` -> `andromeda-core` | Transaction APIs still import through the wide foundation facade. | Replace with extracted transaction foundation imports before widening the exception list. |
| `andromeda-wal` -> `andromeda-core` | WAL owner still imports through the wide foundation facade. | Replace with pure WAL foundation imports before widening the exception list. |
| `andromeda-srpl` -> `andromeda-catalog` | SRPL facade still owns catalog-facing DefinitionBatch and resolver bridges. | Move catalog-facing bridge logic behind a dedicated bridge crate before retiring the exception. |
| `andromeda-srpl` and `andromeda-srpl-parser` -> `andromeda-srpl-lexer` | Tokenization has a dedicated owner crate in the current metadata graph. | Keep lexer catalog-store-free and update topology governance before treating the split as accepted. |
| `andromeda-catalog` -> `andromeda-storage` dev edge | Catalog tests use storage integration harnesses. | Keep dev-only and acyclic; do not promote it into production dependencies without an ADR. |
| `andromeda-observe` -> `andromeda-storage` dev edge | Durable audit storage fixtures use storage test support. | Keep dev-only and ensure audit evidence does not become database truth. |

## Validation

Required validation before accepting manifest changes:

```powershell
cargo metadata --no-deps --format-version 1
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

For C4 or C5 manifest changes, add the relevant package-level compile, property, fuzz, crash/recovery, or threat-model gate from the owning subsystem.

## Troubleshooting

If `cargo metadata` fails, do not update this matrix from memory. Record the failure and fix the manifest or toolchain issue in the owning packet.

If an edge appears that is not covered by ADR-0011 or the topology guard, treat it as a design review item. Do not classify it as allowed until the owning ADR, test, or governance note is updated.

If a dev-dependency forms a back edge, confirm that it is listed as a temporary test harness exception with exit criteria. Otherwise, split the test support into an acyclic helper crate or remove the dependency.

## References

- `AGENTS.md`
- `Cargo.toml`
- `crates/README.md`
- `crates/andromeda-cli/tests/workspace_dependency_topology.rs`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `documentations/architecture/WORKSPACE_RESTRUCTURE_BASELINE_2026.md`
- `documentations/implementation/worktree-packaging-plan-2026-05-08.md`
