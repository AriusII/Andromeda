# Copilot Instructions for Andromeda

## Build, test, and lint commands

Run from repository root.

```powershell
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

Use nextest profiles when validating by runtime class:

```powershell
cargo nextest run --profile default
cargo nextest run --profile ci
cargo nextest run --profile recovery
cargo nextest run --profile wal -p andromeda-wal
```

Run a single integration test target (crate-owned tests are the norm):

```powershell
cargo test -p andromeda-exec --test integration_execution_path --locked -- --nocapture
```

Run a single test case in that test target:

```powershell
cargo test -p andromeda-exec --test integration_execution_path <test_name> --locked -- --exact --nocapture
```

High-signal ownership/topology guard used by this repo:

```powershell
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

## High-level architecture

Andromeda is a procedure-only database engine. The canonical execution path is:

```text
QUIC surface -> typed RPC -> security admission -> contract binding ->
Procedure execution -> transaction scope -> WAL durability -> storage ->
typed ResultStream
```

Key architectural model:

1. Engines own functional domains (catalog/contract, SRPL, execution, transaction, storage, network, optimizer, operations).
2. Cross-cutting planes enforce durability, security, contract compliance, policy, resource governance, observability, and testing evidence.
3. C5 truth boundaries are strict: no visible commit without durable WAL, no RAM-as-truth, and no bypass around typed cataloged Procedures.

Crate structure is intentionally boundary-heavy: many crates are runtime-free contract/codec layers, while runtime integration is isolated in dedicated crates (for example, QUIC runtime split from protocol contracts).

## Key repository conventions

1. No ad hoc SQL application surface and no gRPC introduction; app execution must go through cataloged, typed, versioned Procedure contracts.
2. Keep crate ownership narrow and explicit: one primary responsibility per crate; do not create generic buckets like `common`, `utils`, `misc`, `helpers`, or `god_engine`.
3. Keep `lib.rs` mostly for module declarations and intentional re-exports; keep public API minimal.
4. Library crates should use typed errors; unsafe code must remain private and documented.
5. Never use native Rust struct layout as disk/network format; persistent and wire formats require explicit codecs and validation tests.
6. Do not place executable tests in root `tests/` as ownership for runtime behavior; tests live with owning crates.
7. Prefer deterministic testing. Do not hide flakiness with retries. Use property/fuzz/crash-recovery validation for relevant surfaces.
8. GPU/accelerated paths are strictly excluded from commit, rollback, WAL, recovery, MVCC visibility, and security-critical authorization paths.
9. Use `docs/status.md` for readiness/current-state claims; historical ADR/roadmap content is context, not proof of current readiness.

## Source docs to trust first

1. `docs/README.md` (documentation index and canonical navigation)
2. `docs/architecture/ENGINE_OVERVIEW.md` (big-picture architecture and flow)
3. `docs/project/ANDROMEDA_DOCTRINE.md` (non-negotiable invariants and criticality model)
4. `crates/README.md` and `crates/AGENTS.md` (crate ownership boundaries and validation expectations)
5. `tests/README.md` and `tests/AGENTS.md` (test ownership model and testing behavior)

## MCP servers to configure for this repo

1. **GitHub MCP**: Use for PRs, issues, review threads, workflow runs, job logs, and artifacts while working in `AriusII/Andromeda`.
2. **Rust docs/search MCP**: Use for fast lookups in `docs.rs`, crate APIs, and Rust ecosystem references when changing contract, codec, and runtime-boundary crates.
3. **Playwright MCP**: Keep available for any web UI/docs preview flows that may be added around tooling or dashboards; not for core database runtime validation.
