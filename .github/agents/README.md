# Andromeda custom agents

Catalog of the 13 custom agents available for Copilot CLI work in this repository.

| Agent | Description / when to use |
| --- | --- |
| `andromeda-architecture-orchestrator` | Andromeda architecture governor for workspace, crate, folder, module, and test topology changes; use for architecture, crate boundary, module split, or topology work. |
| `andromeda-implementation-orchestrator` | Primary implementation orchestrator for feature, bugfix, integration, and Rust change requests; use when worker dispatch or multi-step implementation planning is needed. |
| `andromeda-implementation-write-worker` | Bounded Rust implementation worker for scoped feature and bugfix edits with tests; use for direct implementation missions. |
| `andromeda-protocol-contract-worker` | QUIC and custom Protobuf RPC contract worker for frames, ResultStream, IAM, audit, and no-gRPC/no-JSON enforcement; use for protocol changes. |
| `andromeda-readonly-analysis-worker` | Read-only analysis worker for scoped repository investigation, evidence gathering, and Markdown reports; use for analyze/inspect/investigate/report tasks. |
| `andromeda-refactor-orchestrator` | Refactor orchestrator for clean-code, dead-code, orphan, duplicate, file-split, and naming cleanup requests; use for broad cleanup planning. |
| `andromeda-refactor-write-worker` | Bounded refactor worker for clean-code, dead-code, duplicate, file-split, and naming edits; use for scoped refactor missions. |
| `andromeda-release-mission-critical-reviewer` | Read-only mission-critical reviewer for release gates, C4/C5 risk, CI evidence, WAL, security, and blockers; use for readiness reviews. |
| `andromeda-roadmap-orchestrator` | Roadmap orchestrator for docs/roadmap intake, phase selection, staged decomposition, and queued worker execution plans; use for roadmap planning. |
| `andromeda-srpl-catalog-worker` | SRPL, catalog, typed ProcedureContract, DefinitionBatch, binder, and MAPS analytics specialist; use for SRPL/catalog/contract tasks. |
| `andromeda-storage-wal-worker` | WAL, storage, manifest, MVCC, binary codec, and crash-recovery specialist; use for durability and storage changes. |
| `andromeda-test-verification-worker` | Test-topology and verification specialist for cargo, nextest, fuzz, property, Miri, Loom, and recovery evidence; use for validation work. |
| `copilot-tooling-maintainer` | Copilot tooling maintainer for `.github/agents`, `.github/skills`, `.github/hooks`, and `.github/instructions`; use for agent, skill, hook, and instruction authoring. |

## How Copilot CLI loads these

Copilot CLI can select agents through the `/agent` slash command, an explicit `--agent <name>` flag, or inference from the agent descriptions when the task strongly matches a specialty.