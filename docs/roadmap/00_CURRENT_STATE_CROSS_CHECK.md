# Cross-check de l’état actuel Andromeda

## Résumé net

Le dépôt `AriusII/Andromeda`, branche `main`, possède actuellement une base plus avancée que les anciens textes ne le laissent penser. Le `Cargo.toml` courant déclare un workspace Rust 2024 avec `resolver = "3"`, `rust-version = "1.95.0"`, et **89 crates**. Certains documents plus anciens ou le README racine peuvent encore mentionner 88 crates ; cette roadmap retient la vérité de `Cargo.toml`.

## Ce qui est déjà présent

| Domaine | État observé | Lecture roadmap |
|---|---|---|
| Workspace Rust | 89 crates, Rust 1.95.0, Edition 2024, resolver 3. | Baseline technique actuelle. |
| Surface projet | README : `QUIC + custom typed RPC + cataloged Procedure + SRPL + typed ResultStream`. | Doctrine native confirmée. |
| V0 vertical | Inventory/ReserveStock, ProductStock, WAL, ResultStream, recovery tests. | Point d’ancrage P02. |
| WAL | `andromeda-wal` possède FileWal, LSN, durable prefix, fences, transaction classification. | Ne pas réinventer ; intégrer et durcir. |
| Storage | `andromeda-storage` intègre pages, heap, B+Tree, manifest, recovery, WAL-before-page-flush. | Compléter P05. |
| Catalog | `andromeda-catalog` possède descriptors, contracts, DefinitionBatch, catalog recovery, publication evidence. | Durcir P03. |
| SRPL | Crates parser, binder, IR, diagnostics, cardinality, lowering, interpreter, execution adapter. | Structurer P01/P04. |
| QUIC/RPC | `andromeda-quic`, `andromeda-rpc-protocol`, `andromeda-rpc-codec`, runtime Quinn. | Activer après admission stable. |
| IAM/Security | `andromeda-iam` admission pré-transaction ; `andromeda-security` scaffold. | Durabilité et policies en P07. |
| Optimizer/Stats | Crates propriétaires de contrats et DecisionTrace, encore à rendre opérationnels. | P09. |
| Maps/Analytics | Descriptors/evidence runtime-free ; matérialisation à réaliser. | P10. |
| Backup/Restore/HA-DR | Crates partiellement implémentés pour contrats, artefacts locaux et contrôle HA/DR, mais pas readiness. | P13/P14 exigent drills retenus. |

## Contradictions ou écarts à traiter

| Écart | Risque | Décision |
|---|---|---|
| README racine mentionnait 88 crates alors que Cargo en déclare 89. | Documentation readiness fausse. | P00 corrige le README racine et retient Cargo comme source de vérité. |
| `docs/status.md` était référencé par README mais absent lors du fetch GitHub. | Navigation cassée. | P00 recrée `docs/status.md` comme status/current-state document. |
| Roadmap actuelle P0-P11 utile mais générique. | Elle ne distribue pas assez le travail à 20 personnes. | Ce pack étend en 16 phases détaillées. |
| Certains README indiquaient encore scaffold-only alors que backup/restore/HA-DR ont du code contractuel local. | Risque de sous-estimer le périmètre réel ou de surestimer la readiness. | P00 classe ces crates comme partiellement implémentés mais release-blocked. |
| Le vertical V0 prouve certains invariants mais pas encore production runtime. | Claim prématuré. | P02/P15 exigent preuves crash/recovery et release gates. |

## Ligne conductrice

La prochaine étape n’est pas d’ajouter une ambition nouvelle. La prochaine étape est de transformer les éléments déjà présents en chemin durable, générique, récupérable et observable :

```text
Procedure
-> ContractHash
-> CatalogVersion
-> Admission
-> TransactionScope
-> WAL durable
-> Heap/Page durable
-> Recovery
-> ResultStream
-> Procedure Store / Audit evidence
```

## Artefacts P00 de gouvernance

| Artefact | Rôle |
|---|---|
| `docs/status.md` | Snapshot courant : Rust 1.95.0, Edition 2024, resolver 3, 89 crates, readiness boundary. |
| `docs/project/CRATE_CLUSTER_CRITICALITY_MATRIX.md` | Matrice crate -> cluster moteur -> criticité C0-C5. |
| `docs/adr/ADR-0001-RUST_BASELINE_AND_MSRV.md` | ADR baseline Rust. |
| `docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md` | ADR boundaries workspace/crates. |
| `docs/adr/ADR-0004-CANONICAL_BINARY_FORMAT.md` | ADR codecs explicites et no native layout. |
| `docs/adr/ADR-0007-QUIC_RPC_BOUNDARY_NO_GRPC.md` | ADR QUIC/RPC custom et no gRPC. |
| `docs/adr/ADR-0017-NO_DYNAMIC_SQL_APPLICATION_SURFACE.md` | ADR no dynamic/ad hoc SQL application surface. |

## Ce que cette roadmap refuse

- Commencer par GPU ou learned components.
- Ajouter SQL ad hoc pour accélérer la démonstration.
- Créer un God Engine.
- Publier des Maps sans evidence durable.
- Décrire backup/restore/HA-DR comme production-ready tant que les drills ne passent pas.
- Déplacer des comportements C5 dans CLI, observability, benchmark ou GPU.
