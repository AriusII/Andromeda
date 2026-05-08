# Andromeda

Andromeda is a modern relational transactional database project built around a strict native surface:

```text
QUIC + custom typed RPC + cataloged Procedure + SRPL + typed ResultStream
```

It is not a generic SQL server. Application execution must go through cataloged Procedures with typed, hashed, versioned contracts and explicit transaction scope.

## Quick Start

**New to Andromeda?** Start here:

- [Architecture overview](docs/architecture/README.md)
- [ADRs and decisions](docs/adr/README.md)
- [Specifications](docs/specifications/README.md)
- [Operations runbooks](docs/runbooks/README.md)
- [Testing strategy](docs/testing/README.md)

## Repository Layout

| Path | Purpose |
|---|---|
| `crates/` | Rust workspace crates (96 crates total). |
| `docs/` | Architecture decisions, specifications, runbooks, testing guidance. |
| `tools/` | Standalone diagnostic and orchestration tools (xtask, wal-dump, page-dump, etc). |
| `tests/` | Test roadmap index (crate-owned test suites listed by domain). |
| `benches/` | Benchmark infrastructure and scenarios by subsystem. |
| `fuzz/` | Fuzzing targets and seed corpus organization. |
| `supply-chain/` | Dependency governance and security policy. |
| `documentations/` | Project doctrine, roadmap, governance records, implementation status. |
| `.codex/` | Codex operating pack: agents, hooks, workflows, prompt templates, validation. |
| `.agents/` | Reusable skills, agent-facing instructions, governance registries. |
| `.github/` | GitHub Actions, CI scripts, Code Owners, Copilot instructions. |

Root-level source of truth is intentionally small. Generated outputs, runtime logs, local experiments, and one-off reports should stay outside the repository root.

## Documentation Entry Points

Start with the protected current-state and roadmap files:

- [Current state](documentations/CURRENT_STATE.md)
- [Implementation roadmap 2026](documentations/ROADMAP_IMPLEMENTATION_2026.md)
- [Worker execution matrix 2026](documentations/WORKER_EXECUTION_MATRIX_2026.md)
- [Roadmap implementation cross-check](documentations/ANDROMEDA_ROADMAP_IMPLEMENTATION_CROSSCHECK_2026.md)

The consolidated doctrine set remains at the top of `documentations/` and must be treated as the canonical design reference:

- [Index and reading mode](documentations/00_ANDROMEDA_INDEX_ET_MODE_DE_LECTURE.md)
- [Doctrine, lexicon, architecture, catalog, and Modelization](documentations/01_DOCTRINE_LEXIQUE_ARCHITECTURE_CATALOGUE_MODELIZATION.md)
- [Type system, SRPL, Procedures, and Maps](documentations/02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md)
- [Transaction, WAL, MVCC, storage, and recovery](documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md)
- [QUIC, RPC, security, HA/DR, and operations](documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md)
- [Optimizer, statistics, analytics, hardware roadmap, and sources](documentations/05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md)

Supporting documentation is grouped by use:

- [Agent operations](documentations/agent-operations/)
- [Developer guides](documentations/developer-guides/)
- [Governance decisions](documentations/governance/decisions/)
- [Implementation tracking](documentations/implementation/)
- [Operations runbooks](documentations/operations/)
- [References and test vectors](documentations/reference/)

## Operating Pack

Andromeda's local agent tooling is split by responsibility:

| Area | Canonical location |
|---|---|
| Codex agent definitions | `.codex/agents/` |
| Codex hook configuration | `.codex/hooks.json` |
| Codex hook scripts | `.codex/scripts/hooks/` |
| Codex workflows | `.codex/workflows/` |
| Prompt templates | `.codex/templates/prompts/` |
| Tooling schemas | `.codex/schemas/` |
| OpenAI adapter sketches | `.codex/adapters/openai-agents/` |
| Reusable skills | `.agents/skills/` |
| Agent-facing standards | `.agents/instructions/` |
| Governance registries | `.agents/registries/` |

Run this check after editing the operating pack:

```powershell
python .codex/scripts/validate_codex_tooling.py
```

## Local V0 Commands

The Rust workspace includes a local V0 recoverable vertical prototype. It is not yet a production database runtime, complete network server, or complete durable storage engine.

```powershell
cargo run -p andromeda-cli -- vertical-v0 --wal "$env:TEMP\andromeda-v0-vertical.wal"
cargo run -p andromeda-cli -- recovery-inspect "$env:TEMP\andromeda-v0-vertical.wal"
cargo run -p andromeda-cli -- protocol-smoke --detail
```

`vertical-v0` executes the current `Inventory.ReserveStock` path through the local V0 SRPL/FileWal flow and writes a mono-segment WAL file. `recovery-inspect` prints the durable prefix, replay LSNs, ignored transactions, and forensic boundary status for that WAL. `protocol-smoke --detail` checks local payload/frame lockstep and ResultStream ordering without opening network sockets.

## Release And Governance Records

Current readiness must be judged from the active roadmap, current-state file, release-gate workflow, and current tracker output. Historical decision records remain useful governance context, but they do not by themselves prove current readiness.

- [DEC-035: Release gate chain](documentations/governance/decisions/DEC-035-release-gate-chain.md)
- [DEC-036: Release readiness approval](documentations/governance/decisions/DEC-036-release-readiness-approval.md)
- [DEC-037: Risk register updates](documentations/governance/decisions/DEC-037-risk-register-updates.md)
- [DEC-038: B-Tree mutations deferred](documentations/governance/decisions/DEC-038-btree-mutations-deferred.md)
- [DEC-039: Optimizer intermediate pass contract](documentations/governance/decisions/DEC-039-optimizer-intermediate-pass-contract.md)

## Core Validation

Use the narrowest validation that matches the change. For broad Rust changes, start with:

```powershell
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

For documentation and operating-pack changes, use:

```powershell
python .codex/scripts/validate_codex_tooling.py
python .github/scripts/yaml_sanity.py
python -m unittest discover -s .github/scripts/tests
```

## Non-Negotiable Design Constraints

- No ad hoc SQL application surface.
- Every application execution goes through a cataloged Procedure.
- Every Procedure has a typed, hashed, versioned contract.
- Every Procedure is transactionally scoped.
- No visible commit without durable WAL.
- RAM is never system truth.
- GPU never participates in commit, rollback, WAL, recovery, MVCC visibility, or security-critical paths.
- Predictive evidence never decides alone.
- Active plans are tied to `CatalogVersion + StatsVersion + ContractHash`.
- Every critical decision must be observable and explainable after the fact.
