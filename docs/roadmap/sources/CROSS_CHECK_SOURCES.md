# Sources croisées utilisées

## Dépôt GitHub AriusII/Andromeda main

- `Cargo.toml` : workspace Rust 2024, resolver 3, rust-version 1.95.0, 89 crates.
- `README.md` : surface native stricte, Procedure-only, V0 local commands, non-negotiable constraints.
- `docs/README.md` : docs replacement package, baseline Rust 1.95.0, doctrine procedure-only WAL-first.
- `docs/roadmap/ROADMAP.md` : séquence P0-P11 et priorité durable Procedure path.
- `crates/README.md` : crate ownership boundaries, C5 durable-kernel exclusions, no SQL/no gRPC/no runtime JSON default.
- `crates/andromeda-exec/README.md` : execution orchestration, admission, ResultStream, retry, terminal evidence.
- `crates/andromeda-wal/README.md` : FileWal, LSN, durable prefix, fences, WAL byte contract.
- `crates/andromeda-storage/README.md` : pages, heap, B+Tree, buffer pool, manifests, recovery.
- `crates/andromeda-catalog/README.md` : catalog, contracts, DefinitionBatch, recovery, publication evidence.
- `crates/andromeda-srpl/README.md` : compiler orchestration, no ad hoc SQL, typed IR, fixtures.
- `crates/andromeda-quic/README.md` : runtime-free QUIC, surface gating, no gRPC, procedure gateway.
- `crates/andromeda-iam/README.md` et `crates/andromeda-security/README.md` : pre-transaction admission, fail-closed, security boundaries.
- `crates/andromeda-optimizer/README.md` et `crates/andromeda-statistics/README.md` : DecisionTrace, StatsVersion, bounded optimizer policy.
- `crates/andromeda-maps/README.md` : Map descriptors and publication evidence, no Map as source truth.
- `crates/andromeda-backup`, `andromeda-restore`, `andromeda-hadr` : scaffolds réservés, pas readiness.
- `crates/andromeda-inventory-demo/tests/v0_vertical_e2e/*` : ProductStock durable path, WAL recovery, ResultStream metadata, fail-closed gates.

## Documents projet chargés

- `00_ANDROMEDA_INDEX_ET_MODE_DE_LECTURE.md` : doctrine, invariants et lecture globale.
- `01_DOCTRINE_LEXIQUE_ARCHITECTURE_CATALOGUE_MODELIZATION.md` : architecture, catalogue, DefinitionBatch, policies.
- `02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md` : TypeSystem, SRPL, ProcedureContract, Maps.
- `03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md` : Transaction Kernel, WAL, MVCC, Storage, Recovery.
- `04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md` : RPC, surfaces, IAM, audit, HA/DR, backup, restore.
- `05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md` : Procedure Store, optimizer, statistics, GPU/CPU/NVMe/HDD, roadmap.
- `Moteur de stockage sécurisé.txt` : format physique segmenté, root/manifest/WAL/hot/cold/SegmentIndex.
- `Fondation de SRPL pour un langage procédural relationnel strict.pdf` : principes SRPL, set semantics, absence explicite, cardinalité.
- `Rapport consolidé...pdf` et `Corpus de référence...pdf` : socle relationnel, ACID concret, WAL/ARIES, normalisation, statistics, B+Tree, learned components prudents.
- Documents Rust CPU/GPU 2026 : Rust 1.95, unsafe policy, CPU runtime dispatch, GPU batch optional.

## Décisions de cross-check

- La roadmap retient Rust 1.95.0, car le Cargo actuel l’impose.
- La roadmap retient 89 crates, car Cargo.toml est la source de vérité du workspace.
- La roadmap conserve le vertical Inventory.ReserveStock/ProductStock comme preuve P02, mais refuse de l’appeler production runtime.
- La roadmap traite backup/restore/HA-DR comme scaffolds à remplir, pas comme fonctionnalités prêtes.
- La roadmap place GPU après Maps/Analytics CPU et après Resource Governance.
