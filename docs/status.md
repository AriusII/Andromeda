# Andromeda current status

> **Status:** P00 repository truth snapshot
> **Scope:** Current repository state and readiness boundary
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, resolver 3

## Current repository truth

The root `Cargo.toml` is the source of truth for workspace membership during P00.
It currently declares:

```text
Workspace members: 89 crates
Rust version: 1.95.0
Edition: 2024
Resolver: 3
Production toolchain channel: stable
```

This status page supersedes older references that described the workspace as 88
crates. Historical documents may remain useful as context, but they are not
readiness evidence unless they match the current repository and validation
output.

## Readiness boundary

Andromeda currently has a recoverable local V0 vertical path and many
crate-owned contracts, tests, ADRs, and specifications. That does not make the
project production-ready.

Current readiness claims must stay inside these limits:

- the V0 Inventory/ProductStock path is prototype evidence, not a complete
  production runtime;
- backup, restore, and HA/DR crates contain partially implemented contract,
  local artifact, and control-plane boundaries, but recoverability or failover
  claims require retained restore/PITR/ForensicStart and HA/DR drill evidence;
- runtime QUIC, security admission, audit, WAL, storage, recovery, catalog, and
  transaction behavior must be cited with the owning crate tests or retained
  evidence;
- benchmarks, traces, GPU outputs, RAM, and diagnostic outputs are not system
  truth.

## P00 evidence links

| Evidence | File |
|---|---|
| Roadmap authority | `docs/roadmap/ROADMAP_MASTER.md` |
| P00 phase definition | `docs/roadmap/phases/P00_REPOSITORY_STATE_AND_GOVERNANCE.md` |
| Current-state cross-check | `docs/roadmap/00_CURRENT_STATE_CROSS_CHECK.md` |
| Cross-check source list | `docs/roadmap/sources/CROSS_CHECK_SOURCES.md` |
| Crate cluster and criticality matrix | `docs/project/CRATE_CLUSTER_CRITICALITY_MATRIX.md` |
| Rust baseline ADR | `docs/adr/ADR-0001-RUST_BASELINE_AND_MSRV.md` |
| Workspace boundary ADR | `docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md` |
| Canonical binary format ADR | `docs/adr/ADR-0004-CANONICAL_BINARY_FORMAT.md` |
| QUIC/RPC boundary ADR | `docs/adr/ADR-0007-QUIC_RPC_BOUNDARY_NO_GRPC.md` |
| No dynamic SQL ADR | `docs/adr/ADR-0017-NO_DYNAMIC_SQL_APPLICATION_SURFACE.md` |

## Minimum validation for broad changes

```powershell
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

For P00 documentation-only changes, the status claim is limited to the commands
actually run and recorded in the P00 final report.
