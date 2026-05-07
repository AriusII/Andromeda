# Agent and Skill Matrix

| Agent | Sandbox | Primary skills |
|---|---|---|
| `roadmap-master-orchestrator` | `workspace-write` | `roadmap-objective-tracing`, `roadmap-task-decomposition`, `roadmap-delegation-tree`, `roadmap-progress-checkpointing`, `agent-skill-routing` |
| `roadmap-task-decomposer` | `workspace-write` | `roadmap-task-decomposition`, `prompt-scope-normalization`, `agent-handoff-protocol` |
| `roadmap-scope-controller` | `read-only` | `roadmap-objective-tracing`, `user-request-risk-classification`, `source-grounded-answering` |
| `context-intake-analyst` | `read-only` | `project-context-intake`, `prompt-intent-disambiguation`, `source-grounded-answering` |
| `context-pack-builder` | `workspace-write` | `context-pack-construction`, `agent-handoff-protocol`, `source-grounded-answering` |
| `prompt-refinement-specialist` | `workspace-write` | `prompt-scope-normalization`, `codex-prompt-template-design`, `project-context-intake` |
| `codex-routing-architect` | `workspace-write` | `agent-skill-routing`, `codex-agent-authoring`, `codex-agent-registry-maintenance` |
| `codex-skill-maintainer` | `workspace-write` | `codex-skill-authoring`, `skill-progressive-disclosure`, `skill-forward-testing`, `codex-tooling-validation` |
| `hooks-governance-auditor` | `read-only` | `hook-design-governance`, `hook-json-validation`, `permission-request-policy`, `pretooluse-guardrails` |
| `hook-policy-implementer` | `workspace-write` | `hook-design-governance`, `hook-script-hardening`, `hook-json-validation`, `pretooluse-guardrails`, `posttooluse-audit` |
| `agent-registry-maintainer` | `workspace-write` | `codex-agent-registry-maintenance`, `codex-agent-authoring`, `codex-tooling-validation` |
| `source-grounding-researcher` | `read-only` | `source-grounded-answering`, `project-context-intake`, `decision-record-authoring` |
| `documentation-consistency-editor` | `read-only` | `decision-record-authoring`, `source-grounded-answering`, `andromeda-doctrine-invariants` |
| `github-pr-reviewer` | `read-only` | `rust-clean-code-refactor`, `rust-ci-quality-gates`, `release-gate-mission-critical`, `source-grounded-answering` |
| `github-implementation-planner` | `workspace-write` | `project-context-intake`, `roadmap-task-decomposition`, `rust-ci-quality-gates` |
| `rust-architecture-splitter` | `workspace-write` | `rust-workspace-architecture`, `rust-crate-boundary-design`, `rust-module-splitting`, `rust-file-size-governance` |
| `rust-cleanup-refactor-architect` | `workspace-write` | `rust-clean-code-refactor`, `rust-dead-code-removal`, `rust-orphan-detection`, `rust-public-api-minimization` |
| `rust-dead-code-excavator` | `workspace-write` | `rust-dead-code-removal`, `rust-orphan-detection`, `rust-dependency-pruning` |
| `rust-module-boundary-surgeon` | `workspace-write` | `rust-module-splitting`, `rust-public-api-minimization`, `rust-clean-code-refactor` |
| `rust-crate-graph-architect` | `workspace-write` | `rust-workspace-architecture`, `rust-crate-boundary-design`, `rust-dependency-pruning` |
| `rust-performance-optimizer` | `workspace-write` | `rust-performance-benchmarking`, `rust-allocation-review`, `rust-clone-copy-audit`, `cpu-simd-kernel-policy` |
| `rust-allocation-optimizer` | `workspace-write` | `rust-allocation-review`, `rust-clone-copy-audit`, `rust-performance-benchmarking` |
| `rust-async-concurrency-engineer` | `workspace-write` | `rust-async-cancellation`, `rust-concurrency-loom-modeling`, `resultstream-protocol`, `quic-rpc-frame-contracts` |
| `rust-memory-safety-auditor` | `read-only` | `rust-unsafe-boundary-audit`, `rust-miri-ub-checks`, `rust-panic-eradication` |
| `rust-unsafe-audit-agent` | `read-only` | `rust-unsafe-boundary-audit`, `rust-ffi-boundary-audit`, `rust-miri-ub-checks`, `rust-fuzzing-harness-design` |
| `rust-simd-kernel-engineer` | `workspace-write` | `rust-simd-dispatch`, `cpu-simd-kernel-policy`, `rust-performance-benchmarking` |
| `rust-gpu-batch-engineer` | `workspace-write` | `rust-gpu-batch-integration`, `gpu-no-commit-policy`, `rust-performance-benchmarking` |
| `rust-binary-codec-engineer` | `workspace-write` | `rust-newtype-invariants`, `storage-page-format`, `storage-segment-manifest`, `wal-record-design` |
| `rust-error-model-engineer` | `workspace-write` | `rust-error-modeling`, `srpl-diagnostics-catalog`, `quic-rpc-frame-contracts` |
| `test-verification-architect` | `workspace-write` | `rust-test-strategy`, `rust-property-testing`, `rust-fuzzing-harness-design`, `rust-miri-ub-checks`, `crash-injection-matrix` |
| `qa-crash-recovery-test-designer` | `workspace-write` | `wal-crash-recovery-testing`, `crash-injection-matrix`, `forensic-startup-runbook` |
| `transaction-wal-recovery-auditor` | `read-only` | `transaction-state-machine`, `wal-record-design`, `wal-crash-recovery-testing`, `mvcc-isolation-analysis` |
| `wal-implementation-engineer` | `workspace-write` | `wal-record-design`, `wal-crash-recovery-testing`, `rust-binary-codec-design`, `release-gate-mission-critical` |
| `storage-engine-page-layout-auditor` | `read-only` | `storage-page-format`, `storage-segment-manifest`, `hot-cold-storage-policy` |
| `storage-segment-format-engineer` | `workspace-write` | `storage-segment-manifest`, `hot-cold-storage-policy`, `bufferpool-io-scheduler`, `rust-binary-codec-design` |
| `buffer-pool-engineer` | `workspace-write` | `bufferpool-io-scheduler`, `hot-cold-storage-policy`, `observability-decision-trace` |
| `mvcc-isolation-engineer` | `workspace-write` | `mvcc-isolation-analysis`, `transaction-state-machine`, `normalization-dependency-analysis` |
| `srpl-language-specifier` | `read-only` | `srpl-language-design`, `srpl-anti-dynamic-sql`, `relational-algebra-law-check`, `set-theory-semantics` |
| `srpl-parser-binder-engineer` | `workspace-write` | `srpl-parser-design`, `srpl-binder-cardinality`, `srpl-diagnostics-catalog`, `srpl-language-design` |
| `srpl-diagnostics-engineer` | `workspace-write` | `srpl-diagnostics-catalog`, `srpl-binder-cardinality`, `type-system-domain-modeling` |
| `procedure-contract-reviewer` | `read-only` | `srpl-procedure-contract`, `contracthash-canonicalization`, `structuredobject-contracts`, `quic-rpc-frame-contracts` |
| `procedure-contract-implementer` | `workspace-write` | `srpl-procedure-contract`, `contracthash-canonicalization`, `structuredobject-contracts` |
| `catalog-modelization-governor` | `read-only` | `catalog-object-model`, `definitionbatch-validation`, `decision-record-authoring` |
| `modelization-catalog-governor` | `read-only` | `catalog-object-model`, `definitionbatch-validation`, `contracthash-canonicalization` |
| `definitionbatch-implementer` | `workspace-write` | `definitionbatch-validation`, `catalog-object-model`, `audit-trace-schema` |
| `optimizer-statistics-critic` | `read-only` | `optimizer-cost-modeling`, `statistics-histogram-design`, `scenario-evidence-governance`, `learned-component-skepticism` |
| `optimizer-plan-engineer` | `workspace-write` | `optimizer-cost-modeling`, `relational-algebra-law-check`, `observability-decision-trace` |
| `statistics-engineer` | `workspace-write` | `statistics-histogram-design`, `scenario-evidence-governance`, `gpu-no-commit-policy` |
| `procedure-store-engineer` | `workspace-write` | `procedure-store-design`, `observability-decision-trace`, `optimizer-cost-modeling` |
| `map-analytics-summarizability-reviewer` | `read-only` | `map-summarizability-review`, `map-materialization-policy`, `cardinality-grain-analysis` |
| `map-maintenance-engineer` | `workspace-write` | `map-materialization-policy`, `map-summarizability-review`, `wal-record-design` |
| `quic-rpc-contract-auditor` | `read-only` | `quic-rpc-frame-contracts`, `resultstream-protocol`, `pretooluse-guardrails` |
| `quic-rpc-implementation-engineer` | `workspace-write` | `quic-rpc-frame-contracts`, `resultstream-protocol`, `rust-async-cancellation` |
| `security-iam-auditor` | `read-only` | `iam-security-policy`, `security-threat-modeling`, `audit-trace-schema` |
| `security-iam-threat-modeler` | `read-only` | `security-threat-modeling`, `iam-security-policy`, `backup-restore-pitr` |
| `security-implementation-engineer` | `workspace-write` | `iam-security-policy`, `audit-trace-schema`, `security-threat-modeling` |
| `hadr-failover-engineer` | `workspace-write` | `hadr-quorum-fencing`, `backup-restore-pitr`, `audit-trace-schema` |
| `backup-restore-forensic-engineer` | `workspace-write` | `backup-restore-pitr`, `forensic-startup-runbook`, `storage-segment-manifest` |
| `observability-forensic-engineer` | `workspace-write` | `audit-trace-schema`, `observability-decision-trace`, `forensic-startup-runbook` |
| `enterprise-readiness-agent` | `read-only` | `enterprise-readiness-review`, `release-gate-mission-critical`, `backup-restore-pitr` |
| `formal-invariants-agent` | `read-only` | `andromeda-doctrine-invariants`, `relational-algebra-law-check`, `set-theory-semantics`, `transaction-state-machine` |
| `hardware-rust-performance-engineer` | `read-only` | `hardware-profile-policy`, `cpu-simd-kernel-policy`, `gpu-no-commit-policy`, `rust-performance-benchmarking` |
| `devex-rust-tooling-agent` | `workspace-write` | `rust-ci-quality-gates`, `codex-tooling-validation`, `prompt-library-maintenance` |
| `dependency-supply-chain-auditor` | `read-only` | `rust-supply-chain-audit`, `rust-dependency-pruning`, `rust-feature-flag-governance` |
| `ci-quality-gate-engineer` | `workspace-write` | `rust-ci-quality-gates`, `codex-tooling-validation`, `hook-json-validation` |
| `mcp-awareness-integrator` | `workspace-write` | `mcp-aware-tool-selection`, `context-pack-construction`, `rust-workspace-architecture` |
| `prompt-library-curator` | `workspace-write` | `prompt-library-maintenance`, `codex-prompt-template-design`, `agent-skill-routing` |
| `release-readiness-coordinator` | `workspace-write` | `release-gate-mission-critical`, `enterprise-readiness-review`, `rust-ci-quality-gates` |
| `codebase-cleanup-commander` | `workspace-write` | `rust-clean-code-refactor`, `rust-dead-code-removal`, `rust-orphan-detection`, `rust-file-size-governance` |
| `mega-refactor-coordinator` | `workspace-write` | `roadmap-task-decomposition`, `rust-clean-code-refactor`, `agent-handoff-protocol`, `rust-ci-quality-gates` |
| `file-size-split-enforcer` | `workspace-write` | `rust-file-size-governance`, `rust-module-splitting`, `rust-public-api-minimization` |
| `orphan-detection-specialist` | `workspace-write` | `rust-orphan-detection`, `rust-dead-code-removal`, `rust-dependency-pruning` |
