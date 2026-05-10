# Andromeda documentation index

> **Status:** Navigation map  
> **Scope:** Replacement `docs/` folder  
> **Baseline:** Rust 1.95.0

## In this article

- Choose a reading path.
- Understand the folder structure.
- Locate specifications, ADRs, runbooks, and test plans.
- Use the roadmap with sequence-only language.

## Folder structure

```text
docs/
  README.md
  status.md
  INDEX.md
  DOCS_MANIFEST.md
  STYLE_GUIDE.md
  project/
  architecture/
  specifications/
  adr/
  runbooks/
  testing/
  roadmap/
  operations/
  reference/
  templates/
```

## Recommended reading paths

### Architecture path

1. `project/ANDROMEDA_DOCTRINE.md`
2. `architecture/ENGINE_OVERVIEW.md`
3. `architecture/FOUR_ENGINE_MODEL.md`
4. `architecture/STORAGE_ARCHITECTURE.md`
5. `architecture/QUIC_RPC_SECURITY_ARCHITECTURE.md`
6. `architecture/OBSERVABILITY_ARCHITECTURE.md`

### Language and contract path

1. `architecture/SRPL_TYPE_SYSTEM_ARCHITECTURE.md`
2. `specifications/SPEC_TYPE_SYSTEM_V0.md`
3. `specifications/SPEC_PROCEDURE_CONTRACT_V0.md`
4. `specifications/SPEC_SRPL_GRAMMAR_V0.md`
5. `specifications/SPEC_SRPL_BINDER_V0.md`
6. `specifications/SPEC_SEMANTIC_IR_V0.md`
7. `specifications/SPEC_CATALOG_OBJECT_MODEL_V0.md`
8. `specifications/SPEC_DEFINITION_BATCH_V0.md`

### Transaction and storage path

1. `architecture/TRANSACTION_ARCHITECTURE.md`
2. `architecture/STORAGE_ARCHITECTURE.md`
3. `specifications/SPEC_TRANSACTION_STATE_MACHINE_V0.md`
4. `specifications/SPEC_WAL_RECORD_V0.md`
5. `specifications/SPEC_FILE_WAL_SEGMENT_V0.md`
6. `specifications/SPEC_PAGE_FORMAT_V0.md`
7. `specifications/SPEC_DATABASE_MANIFEST_V0.md`
8. `specifications/SPEC_SEGMENT_INDEX_V0.md`
9. `specifications/SPEC_RECOVERY_REPORT_V0.md`
10. `specifications/SPEC_CRASH_RECOVERY_TEST_PLAN_V0.md`
11. `testing/CRASH_RECOVERY_TEST_PLAN.md`

### Security and operations path

1. `architecture/QUIC_RPC_SECURITY_ARCHITECTURE.md`
2. `specifications/SPEC_RPC_FRAME_V0.md`
3. `specifications/SPEC_RESULT_STREAM_V0.md`
4. `specifications/SPEC_SECURITY_ADMISSION_V0.md`
5. `specifications/SPEC_AUDIT_LEDGER_V0.md`
6. `specifications/SPEC_DECISION_TRACE_V0.md`
7. `operations/BACKUP_RESTORE_PITR.md`
8. `operations/HADR_AND_CLUSTER_OPERATIONS.md`
9. `runbooks/RUNBOOK_FORENSIC_START.md`

### Roadmap path

1. `roadmap/ROADMAP_MASTER.md`
2. `roadmap/phases/P00_REPOSITORY_STATE_AND_GOVERNANCE.md`
3. `roadmap/phases/P01_NORMATIVE_SPECIFICATION_BASELINE.md`
4. Continue phase files in order.

## Main folders

| Folder | Role |
|---|---|
| `project/` | Doctrine, glossary, criticality, crate matrix, feature acceptance, Rust baseline, source cross-check. |
| `architecture/` | System structure, engine clusters, storage, transaction, SRPL, RPC, observability, repository architecture. |
| `specifications/` | Normative V0 specs. Each spec includes invariants, structures, error model, recovery behavior, tests, and rejection criteria. |
| `adr/` | Explicit decisions that constrain implementation and future changes. |
| `runbooks/` | Operational procedures for incidents and maintenance. |
| `testing/` | Test strategy, CI gates, crash tests, fuzzing, property tests, and release gates. |
| `roadmap/` | Sequenced work with sequence-only language. |
| `operations/` | Backup, restore, HA/DR, deployment, metrics, and observability operations. |
| `reference/` | Source basis, terminology, and documentation standards. |
| `templates/` | Reusable Markdown skeletons. |

## Rule for documentation changes

A documentation change that changes architecture, invariants, safety policy, storage format, security model, or roadmap sequencing must update at least one of these files:

```text
project/ANDROMEDA_DOCTRINE.md
project/FEATURE_ACCEPTANCE_GATE.md
adr/ADR-*.md
specifications/SPEC_*.md
roadmap/ROADMAP_MASTER.md
```
