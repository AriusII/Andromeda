# Andromeda AI Operating Pack 2026

**Version:** 0.1.0  
**Date:** 2026-05-03  
**Language:** American English  
**Style:** Microsoft documentation style  
**Project:** Andromeda — Modern Transactional Relational Database System, SRPL, Rust, QUIC, Protobuf, storage hot/cold,
deterministic and mission-critical design.

## Purpose

This archive provides a project-specific AI operating layer for Andromeda. It includes reusable instructions, agent
definitions, focused skills, hooks, prompt templates, quality gates, schemas, and workflows.

The pack is intentionally strict. It exists to keep AI-assisted work aligned with the Andromeda doctrine:

- Procedure-only execution.
- No ad hoc SQL application surface.
- QUIC transport.
- Protobuf contracts without gRPC.
- Strong typing, deterministic behavior, versioned contracts, and observable decisions.
- Rust-first implementation with controlled `unsafe`.
- WAL-before-visible-commit transaction semantics.
- Recovery, audit, and security before optimization.

## How to use this archive

1. Start with `docs/01_AI_OPERATING_ARCHITECTURE.md`.
2. Review `instructions/` and select the project instruction set that matches the workstream.
3. Use `.claude/agents/` as specialized subagent definitions.
4. Use `.claude/skills/` as focused model-invoked capabilities.
5. Use `.claude/settings.json` and `hooks/scripts/` as a guarded Claude Code hook baseline.
6. Use `prompts/` for task briefs and repeatable review requests.
7. Use `registries/` to understand which agents consume which skills and which hooks guard which workflows.

## Local V0 Rust commands

The Rust workspace includes a local V0 recoverable vertical prototype. It is not a production database runtime, network
server, or full storage engine.

```powershell
cargo run -p andromeda-cli -- vertical-v0 --wal "$env:TEMP\andromeda-v0-vertical.wal"
cargo run -p andromeda-cli -- recovery-inspect "$env:TEMP\andromeda-v0-vertical.wal"
cargo run -p andromeda-cli -- protocol-smoke --detail
```

`vertical-v0` executes the current `Inventory.ReserveStock` path through the local V0 SRPL/FileWal flow and writes a
mono-segment WAL file. `recovery-inspect` prints the durable prefix, replay LSNs, ignored transactions, and forensic
boundary status for that WAL. `protocol-smoke --detail` checks local payload/frame lockstep and result-stream ordering
without opening network sockets.

## Design rule

Do not copy a full agent prompt into a skill. Agents orchestrate work. Skills provide narrow repeatable procedures.
Hooks enforce policy boundaries.

## Contents

| Area                   | Path                                      | Purpose                                                                     |
|------------------------|-------------------------------------------|-----------------------------------------------------------------------------|
| Research basis         | `docs/00_RESEARCH_BASIS.md`               | Source-grounded basis for prompts, agents, hooks, and skills.               |
| Operating architecture | `docs/01_AI_OPERATING_ARCHITECTURE.md`    | Explains how instructions, agents, skills, and hooks fit together.          |
| Instructions           | `instructions/`                           | Stable cross-agent policies and standards.                                  |
| Agents                 | `.claude/agents/`                         | Specialized AI workers for Andromeda workstreams.                           |
| Skills                 | `.claude/skills/`                         | Focused capabilities with discoverable `SKILL.md` files.                    |
| Hooks                  | `.claude/settings.json`, `hooks/scripts/` | Lifecycle guardrails for prompts, tool calls, outputs, and stop conditions. |
| Prompts                | `prompts/`                                | Reusable prompt templates.                                                  |
| Workflows              | `workflows/`                              | Repeatable end-to-end operating procedures.                                 |
| Registries             | `registries/`                             | Agent-skill-hook mapping.                                                   |
| Schemas                | `schemas/`                                | JSON schemas for validating future pack assets.                             |

## Important limitations

This pack is not a substitute for formal engineering review, Rust compilation, fuzz testing, crash-recovery testing,
cryptographic review, or database correctness proofs. It is an AI operating layer intended to increase consistency,
reduce drift, and keep work aligned with the project doctrine.

## Wave 14 status and implementation package

### Release status

- Wave 14 release governance is locked and approved.
- Blocking release gates: **7/7 passed**.
- Audit families: **8/8 complete**.
- Completion handoff: `docs/WAVE_14_COMPLETION_HANDOFF.md`.

### Wave 14 feature summary

Wave 14 finalizes release-governance and verification boundaries for:

- Crash recovery determinism and replay convergence.
- Durable visibility rules and transaction/WAL fences.
- ContractHash rejection before transaction creation.
- Audit-family completeness and durable audit sink behavior.
- Critical-path panic prevention and typed error boundaries.
- Published cold segment immutability and corruption boundaries.
- Reproducible crash/restart behavior with deterministic evidence.

### Wave 14 decision records

- [DEC-035: Wave 14 release gates](docs/decisions/DEC-035-wave-14-release-gates.md) *(master gate index)*
- [DEC-036: Wave 14 release approval](docs/decisions/DEC-036-wave-14-release-approval.md)
- Related Wave 13/14 governance context:
  - [DEC-037: Wave 13 risk updates](docs/decisions/DEC-037-wave-13-risk-updates.md)
  - [DEC-038: Wave 13 B-Tree mutations deferred](docs/decisions/DEC-038-wave13-btree-mutations-deferred.md)
  - [DEC-039: Wave 13/14 optimizer intermediate pass contract](docs/decisions/DEC-039-wave13-optimizer-intermediate-pass-contract.md)

### Quick start for implementers

1. Read `docs/WAVE_14_IMPLEMENTER_QUICKSTART.md`.
2. Read DEC-035 and DEC-036 before implementation selection.
3. Run verification commands from repository root.
4. Select a Wave 15 domain and implement smallest end-to-end vertical increment first.

### Verification commands

```powershell
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```

## Skills inventory and purpose

The repository currently exposes **53 skills** (including the requested Wave governance skill set).
Primary purpose per skill:

| Skill | Purpose |
|---|---|
| `access-path-index-review` | Review access path design for B+Tree, hash, hot/cold merge, and scan requirements. |
| `agent-handoff-contracting` | Write precise handoff contracts between agents. |
| `agent-output-validation` | Validate final agent outputs against task completion and project gates. |
| `audit-trace-specification` | Specify audit trace fields for security, catalog, transaction, recovery, or agent operations. |
| `backup-pitr-runbook` | Write or review backup and point-in-time recovery runbooks. |
| `benchmark-workload-design` | Design bounded benchmark workloads and evidence scenarios. |
| `catalog-object-modeling` | Model catalog objects, versions, dependencies, and contract hashes. |
| `crash-recovery-test-design` | Design crash and recovery test scenarios. |
| `decision-record-authoring` | Author decision records for durable architecture or policy changes. |
| `definition-batch-dryrun` | Design or review DefinitionBatch dry-run and apply behavior. |
| `gpu-off-commit-path-check` | Verify that GPU usage remains outside commit, WAL, rollback, and recovery paths. |
| `hadr-quorum-review` | Review HA/DR quorum, fencing, failover, and promotion rules. |
| `hook-policy-design` | Design hook policies for prompt, tool, artifact, and stop gates. |
| `hotcold-data-placement` | Classify data placement across RAM, HotStore NVMe, and ColdStore HDD. |
| `map-refresh-policy` | Define Map consistency and refresh mode. |
| `microsoft-doc-style-edit` | Edit documentation to Microsoft documentation style. |
| `mvcc-visibility-proof` | Review MVCC visibility rules for deterministic snapshot behavior. |
| `no-grpc-enforcement` | Scan artifacts for accidental gRPC introduction. |
| `no-json-runtime-policy` | Scan runtime protocol artifacts for JSON default format drift. |
| `no-sql-surface-scan` | Scan artifacts for accidental ad hoc SQL surface introduction. |
| `observability-decision-trace` | Define DecisionTrace and observability fields for plan, recovery, and admin decisions. |
| `optimizer-planclass-design` | Design bounded multi-plan classes and plan cache behavior. |
| `performance-budgeting` | Define performance budgets and regression thresholds. |
| `permission-policy-matrix` | Build or review permission and policy matrices. |
| `predictive-evidence-scoring` | Define ScenarioEvidence scoring and optimizer consumption rules. |
| `project-invariant-check` | Check an artifact against Andromeda non-negotiable invariants. |
| `prompt-injection-threat-model` | Threat-model prompt injection and agentic manipulation. |
| `property-fuzz-test-design` | Design property-based and fuzz tests for binary, parser, and protocol surfaces. |
| `protobuf-schema-review` | Review Protobuf schemas for Andromeda contract compatibility. |
| `quic-frame-design` | Design QUIC frame mapping for Andromeda RPC. |
| `recovery-replay-proof` | Prove or review recovery replay procedure. |
| `risk-register-update` | Update risks with severity, likelihood, mitigation, owner, and status. |
| `rowcount-metadata-design` | Specify exact row count metadata for procedure results and StructuredObjects. |
| `rust-async-quic-review` | Review async Rust and QUIC code or design for cancellation, backpressure, and lifetimes. |
| `rust-core-code-review` | Review Rust core engine code for correctness, safety, and Andromeda invariants. |
| `rust-crate-boundary-design` | Design Rust crate boundaries aligned with engine modules. |
| `rust-unsafe-audit` | Audit unsafe Rust plans or code for memory, aliasing, and lifetime invariants. |
| `scientific-crosscheck` | Cross-check technical claims against source classes and project doctrine. |
| `security-mtls-iam-review` | Review mTLS, certificate identity, UserPrincipal, and permissions. |
| `segment-contiguity-audit` | Audit segment and page layouts for cold contiguity invariants. |
| `skill-composition-review` | Review whether skills are atomic and correctly composed by agents. |
| `source-corpus-synthesis` | Synthesize project and external research sources into decision-ready notes. |
| `srpl-cardinality-typing` | Review SRPL expressions for explicit type and cardinality semantics. |
| `srpl-error-semantics` | Define SRPL failure, rollback, poison, and error return behavior. |
| `srpl-procedure-contract-design` | Design strict SRPL procedure signatures and contracts. |
| `srpl-to-ir-lowering` | Lower SRPL constructs to typed AST and relational IR requirements. |
| `statistics-histogram-design` | Design statistics, histograms, skew detection, and StatsVersion publication. |
| `storage-page-layout` | Design page header, payload, slot directory, and trailer layouts. |
| `structuredobject-layout` | Design StructuredObject row, column, or hybrid layouts. |
| `terminology-normalization` | Normalize language to Andromeda native terminology and controlled SQL equivalences. |
| `test-matrix-generation` | Generate test matrices for specs, code, protocols, and recovery scenarios. |
| `transaction-state-machine` | Define or review transaction state transitions. |
| `wal-record-design` | Design WAL record types and durability semantics. |
