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
| Backup/Restore/HA-DR | Scaffolds réservés, pas readiness. | P13/P14. |

## Contradictions ou écarts à traiter

| Écart | Risque | Décision |
|---|---|---|
| README racine mentionne 88 crates alors que Cargo en déclare 89. | Documentation readiness fausse. | P00 corrige ou note explicitement l’écart. |
| `docs/status.md` référencé par README mais absent lors du fetch GitHub. | Navigation cassée. | P00 recrée un status/current-state document. |
| Roadmap actuelle P0-P11 utile mais générique. | Elle ne distribue pas assez le travail à 20 personnes. | Ce pack étend en 16 phases détaillées. |
| Plusieurs crates sont scaffolds. | Risque de surestimer readiness. | Chaque scaffold a phase dédiée et exit criteria. |
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

## Ce que cette roadmap refuse

- Commencer par GPU ou learned components.
- Ajouter SQL ad hoc pour accélérer la démonstration.
- Créer un God Engine.
- Publier des Maps sans evidence durable.
- Décrire backup/restore/HA-DR comme production-ready tant que les drills ne passent pas.
- Déplacer des comportements C5 dans CLI, observability, benchmark ou GPU.
