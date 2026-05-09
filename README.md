# Andromeda

Andromeda is a modern relational transactional database project built around a
strict native surface:

```text
QUIC + custom typed RPC + cataloged Procedure + SRPL + typed ResultStream
```

It is not a generic SQL server. Application execution must go through cataloged
Procedures with typed, hashed, versioned contracts and explicit transaction
scope.

## Start Here

`/docs` is the canonical documentation entrypoint.

- [Documentation index](docs/README.md)
- [Project status](docs/status.md)
- [Architecture overview](docs/architecture/README.md)
- [Domain specifications](docs/specs/README.md)
- [Operations runbooks](docs/runbooks/README.md)
- [Testing strategy](docs/testing/README.md)

## Repository Layout

| Path | Purpose |
| --- | --- |
| `crates/` | Rust workspace crates (88 crates total). |
| `docs/` | Canonical architecture, specification, governance, implementation, runbook, and testing documentation. |
| `tests/` | Repository-level test indexes and shared test documentation. |
| `fuzz/` | Fuzzing targets and corpus organization. |
| `tools/` | Repository tooling and validation helpers. |
| `.cargo/` | Cargo configuration. |
| `.config/` | Tool configuration, including nextest profiles. |

Root-level source of truth is intentionally small. Generated outputs, runtime
logs, local experiments, and one-off reports should stay outside the repository
root.

## Documentation

Use [docs/README.md](docs/README.md) for navigation. Use
[docs/status.md](docs/status.md) before citing implementation readiness,
documentation status, or current crate counts.

New cross-links should target `/docs`.

## Current Status

- The workspace currently has 88 active crates.
- `/docs` is the canonical documentation surface.
- The local V0 vertical path remains a prototype, not a production database
  runtime, complete network server, or complete durable storage engine.
- Active documentation and tooling links use `/docs` and `tools/loom-models`.

## Local V0 Commands

The Rust workspace includes a local V0 recoverable vertical prototype:

```powershell
cargo run -p andromeda-cli -- vertical-v0 --wal "$env:TEMP\andromeda-v0-vertical.wal"
cargo run -p andromeda-cli -- recovery-inspect "$env:TEMP\andromeda-v0-vertical.wal"
cargo run -p andromeda-cli -- protocol-smoke --detail
```

`vertical-v0` executes the current `Inventory.ReserveStock` path through the
local V0 SRPL/FileWal flow and writes a mono-segment WAL file.
`recovery-inspect` prints the durable prefix, replay LSNs, ignored
transactions, and forensic boundary status for that WAL. `protocol-smoke
--detail` checks local payload/frame lockstep and ResultStream ordering without
opening network sockets.

## Release And Governance

Current readiness must be judged from status docs, release-gate evidence, and
current validation output. Historical decision records remain useful governance
context, but they do not by themselves prove current readiness.

- [Architecture decision records](docs/adr/README.md)
- [Governance documents](docs/governance/README.md)
- [Implementation roadmap](docs/implementation/roadmap.md)
- [Extraction status](docs/implementation/extraction-status.md)

## Core Validation

Use the narrowest validation that matches the change. For broad Rust changes,
start with:

```powershell
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

For documentation-only edits, use targeted link, status, and terminology
checks before broader Rust validation.

## Non-Negotiable Design Constraints

- No ad hoc SQL application surface.
- Every application execution goes through a cataloged Procedure.
- Every Procedure has a typed, hashed, versioned contract.
- Every Procedure is transactionally scoped.
- No visible commit without durable WAL.
- RAM is never system truth.
- GPU never participates in commit, rollback, WAL, recovery, MVCC visibility,
  or security-critical paths.
- Predictive evidence never decides alone.
- Active plans are tied to `CatalogVersion + StatsVersion + ContractHash`.
- Every critical decision must be observable and explainable after the fact.
