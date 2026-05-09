# Charte V2 — Qualité, Clean Code, Refactorisation et Architecture Rust 2026

> **Projet cible** : Andromeda / moteur SGBDRT moderne 2026  
> **Nature** : guide de cadrage, charte qualité, mode d’emploi de clean-up/refactorisation, standard d’architecture Rust  
> **Version** : V2 consolidée  
> **Date de consolidation** : 2026-05-06  
> **Intention** : produire une base Rust professionnelle, durable, refactorisable, testable, auditée, mesurable et exploitable dans un contexte moteur système / base de données / réseau / stockage.

---

## 0. Résumé exécutif

Ce document transforme l’intention initiale de qualité Rust en une charte exploitable pour un projet sérieux, multi-crates, long terme, orienté moteur système.

L’objectif n’est pas seulement de « nettoyer du code ». L’objectif est de construire une base Rust où chaque fichier, module, crate, dépendance, feature, test, benchmark, script et document possède une raison d’exister, une responsabilité explicite et une vérification possible.

La doctrine centrale est la suivante :

```text
Un code Rust professionnel est un système gouverné.
Il doit être typé, borné, mesuré, testé, documenté, refactorisable, observable et supprimable.
```

Une base de code saine doit donc :

- réduire activement la dette structurelle ;
- supprimer le code mort et les éléments orphelins ;
- éviter les crates fourre-tout ;
- éviter les modules « utilitaires » sans cohérence ;
- contrôler la visibilité publique ;
- séparer la logique pure des effets de bord ;
- rendre les invariants explicites par les types ;
- interdire les optimisations non mesurées ;
- auditer l’usage d’`unsafe` ;
- standardiser les quality gates ;
- garder une architecture lisible même lorsque le volume de code augmente.

Pour Andromeda, cette charte doit être lue comme un standard d’ingénierie Rust au service d’un moteur transactionnel critique. Cela implique une rigueur supérieure à un projet applicatif classique : WAL, storage, RPC, catalogue, sécurité, recovery, statistiques et optimiseur ne doivent pas être seulement « codés ». Ils doivent être isolés, vérifiables, crash-testés et maintenus par des frontières de crates nettes.

---

## 1. Périmètre de la charte

Cette charte couvre :

- l’organisation d’un workspace Rust en 2026 ;
- les conventions de crates, modules, fichiers et dossiers ;
- la politique de clean code ;
- la politique de refactorisation ;
- la suppression du code mort ;
- la détection des fichiers, modules, features et dépendances orphelins ;
- la consolidation des responsabilités ;
- les règles de découpage de fichiers trop lourds ;
- les règles de split en sous-modules ;
- la gestion des dépendances Cargo ;
- la gouvernance des features ;
- la qualité exécutable via CI et quality gates ;
- les tests unitaires, intégration, propriété, fuzz, crash/recovery et concurrence ;
- la performance mesurée ;
- la sécurité mémoire ;
- l’usage contrôlé de `unsafe` ;
- la documentation technique ;
- la review d’architecture ;
- la Definition of Done ;
- l’application concrète à une architecture Andromeda.

Cette charte ne remplace pas :

- les spécifications fonctionnelles du moteur ;
- les documents SRPL ;
- les documents WAL/MVCC/recovery ;
- les décisions d’architecture détaillées ;
- les ADR/DEC formels du dépôt.

Elle sert de **cadre transversal de qualité Rust**.

---

## 2. Baseline Rust 2026

### 2.1 Baseline recommandée

Pour un projet Rust sérieux en 2026, la baseline recommandée est :

```text
Rust stable récent
Edition Rust 2024
Cargo resolver = "3"
rust-version explicite
rust-toolchain.toml versionné
rustfmt.toml versionné
workspace.lints centralisés
CI stricte et reproductible
```

Au 06 mai 2026, la recherche effectuée identifie Rust `1.95.0` comme version stable récente publiée par l’équipe Rust. Cette information doit être utilisée comme point de départ, mais le dépôt doit toujours documenter explicitement la version effectivement retenue.

### 2.2 Edition 2024

Le projet doit viser :

```toml
edition = "2024"
```

Raisons :

- langage et style alignés sur l’état moderne de Rust ;
- meilleur alignement avec le resolver Cargo moderne ;
- migration explicite plutôt que dérive implicite ;
- réduction du risque de comportements divergents entre contributeurs.

### 2.3 Resolver Cargo

Dans un workspace virtuel, déclarer explicitement :

```toml
[workspace]
resolver = "3"
```

Justification :

- le resolver est global au workspace ;
- l’édition 2024 implique une résolution plus consciente de `rust-version` ;
- les grands workspaces doivent éviter les comportements implicites.

### 2.4 MSRV / rust-version

Chaque crate publique ou durable doit déclarer :

```toml
[package]
rust-version = "1.95"
```

ou la version minimale réellement testée.

Règles :

| Cas | Politique |
|---|---|
| Crate interne strictement workspace | `rust-version` aligné workspace. |
| Crate publiée ou réutilisable | MSRV documentée et testée. |
| Crate expérimentale | MSRV alignée toolchain pinée, statut `experimental`. |
| Crate critique C5 | MSRV stricte, testée en CI. |

### 2.5 rust-toolchain.toml

Le dépôt doit versionner :

```toml
[toolchain]
channel = "1.95.0"
components = [
    "rustfmt",
    "clippy",
    "rust-src",
    "llvm-tools-preview"
]
targets = [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu"
]
profile = "default"
```

Adaptations possibles :

- utiliser `stable` pour un dépôt plus flexible ;
- pinner précisément une version pour reproductibilité forte ;
- utiliser nightly uniquement pour des tâches séparées comme `miri`, `cargo-udeps`, fuzzing avancé ou certaines expérimentations.

### 2.6 rustfmt.toml

Versionner :

```toml
style_edition = "2024"
edition = "2024"
```

Raison : éviter les diffs parasites entre CI, IDE et développeurs.

### 2.7 Cargo.lock

Politique recommandée :

| Type de dépôt | `Cargo.lock` |
|---|---|
| Application / binaire / moteur | Commit obligatoire. |
| Workspace interne Andromeda | Commit obligatoire. |
| Bibliothèque publiée uniquement | Selon stratégie, mais recommandé si CI workspace. |
| Crate expérimentale isolée | Commit recommandé. |

Pour Andromeda, `Cargo.lock` doit être suivi.

---

## 3. Glossaire qualité

### 3.1 Clean code

Code lisible, localement compréhensible, typé, testé, nommé correctement, sans surprise inutile et sans dépendance implicite.

### 3.2 Refactorisation

Transformation interne du code sans changement volontaire du comportement observable.

Une refactorisation valide doit :

- préserver les tests existants ;
- clarifier la structure ;
- réduire la duplication ou le couplage ;
- améliorer la testabilité ;
- ne pas introduire d’abstraction spéculative ;
- être relue comme changement structurel.

### 3.3 Clean-up

Suppression ou correction de ce qui n’a plus de rôle :

- code mort ;
- imports inutiles ;
- warnings ;
- fichiers non référencés ;
- modules orphelins ;
- dépendances inutilisées ;
- features abandonnées ;
- exemples cassés ;
- benchmarks inutilisables ;
- documentation périmée.

### 3.4 Consolidation

Regroupement contrôlé d’éléments qui représentent le même concept.

La consolidation n’est pas une fusion aveugle. Elle doit nommer un concept stable.

### 3.5 Élément orphelin

Élément présent dans le dépôt mais non relié à un usage réel, vérifié ou documenté.

Exemples :

- fichier `.rs` non déclaré par `mod`;
- crate non référencée ;
- feature jamais testée ;
- fixture inutilisée ;
- script non appelé ;
- benchmark non lancé ;
- document d’architecture dépassé ;
- TODO historique sans issue ;
- module de compatibilité sans consommateur.

### 3.6 Dette technique

Écart connu entre l’état actuel et l’état souhaitable, accepté temporairement, documenté et suivi.

Une dette silencieuse n’est pas une dette : c’est une dérive.

### 3.7 Surface publique

Tout élément accessible au-delà de son périmètre naturel :

- `pub`;
- API crate ;
- feature Cargo ;
- type exporté ;
- format de fichier ;
- frame réseau ;
- erreur exposée ;
- comportement documenté ;
- contrat de procédure.

Une surface publique coûte cher. Elle doit être rare, testée et documentée.

---

## 4. Principes directeurs

## 4.1 Lisibilité structurelle

Le code doit permettre de répondre rapidement :

```text
Où est la responsabilité ?
Qui appelle ?
Qui possède la donnée ?
Qui peut muter ?
Qui persiste ?
Qui sérialise ?
Qui trace ?
Qui décide ?
Qui teste ?
```

Si ces réponses demandent de traverser dix fichiers sans logique stable, l’architecture doit être revue.

## 4.2 Typage fort

Utiliser des types métier plutôt que des alias primitifs lorsque l’invariant compte.

Préférer :

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SegmentId(u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractHash([u8; 32]);
```

À éviter :

```rust
pub type PageId = u64;
pub type ContractHash = String;
```

Un alias ne protège presque rien. Un newtype protège l’intention.

## 4.3 Responsabilité unique

Un module doit avoir une seule raison principale de changer.

Exemples de mauvais signaux :

- `utils.rs` contenant parsing, I/O, date, retry, checksum, logging ;
- `core.rs` contenant tout ce qui n’a pas trouvé sa place ;
- `mod.rs` de 900 lignes ;
- `lib.rs` avec logique d’exécution ;
- `error.rs` global contenant toutes les erreurs du monde ;
- crate `common` importée partout.

## 4.4 Visibilité minimale

Par défaut :

```rust
private
```

Puis, seulement si nécessaire :

```rust
pub(super)
pub(crate)
pub
```

Règles :

| Visibilité | Signification |
|---|---|
| privé | détail local, choix par défaut. |
| `pub(super)` | détail partagé avec le parent immédiat. |
| `pub(crate)` | détail interne à la crate. |
| `pub` | contrat public durable. |

Une visibilité trop large est une dette d’architecture.

## 4.5 Mesure avant optimisation

Aucune optimisation importante ne doit être acceptée sans :

- hypothèse ;
- mesure avant ;
- modification ;
- mesure après ;
- comparaison ;
- impact mémoire ;
- impact compilation ;
- impact lisibilité ;
- risque introduit.

Formule de décision :

```text
Optimisation acceptée = gain mesuré + complexité acceptable + tests suffisants + rollback possible
```

## 4.6 Suppression active

Le dépôt ne doit pas accumuler du code « au cas où ».

Règle :

```text
Un élément sans usage réel, sans test, sans propriétaire et sans issue doit être supprimé.
```

## 4.7 Documentation utile

La documentation doit expliquer :

- pourquoi ce module existe ;
- quels invariants il protège ;
- quelles erreurs il peut produire ;
- quelles limites il assume ;
- quelles alternatives ont été rejetées ;
- ce qui est stable et ce qui ne l’est pas.

La documentation ne doit pas simplement paraphraser le code.

---

## 5. Organisation du dépôt

### 5.1 Structure générale recommandée

```text
andromeda/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── rustfmt.toml
├── deny.toml
├── README.md
├── CONTRIBUTING.md
├── ARCHITECTURE.md
├── SECURITY.md
├── QUALITY.md
├── crates/
│   ├── andromeda-core/
│   ├── andromeda-types/
│   ├── andromeda-catalog/
│   ├── andromeda-contract/
│   ├── andromeda-srpl/
│   ├── andromeda-exec/
│   ├── andromeda-tx/
│   ├── andromeda-wal/
│   ├── andromeda-storage/
│   ├── andromeda-buffer/
│   ├── andromeda-rpc/
│   ├── andromeda-quic/
│   ├── andromeda-security/
│   ├── andromeda-observability/
│   ├── andromeda-optimizer/
│   ├── andromeda-stats/
│   ├── andromeda-cli/
│   └── andromeda-testing/
├── tests/
│   ├── integration/
│   ├── recovery/
│   ├── protocol/
│   ├── fixtures/
│   └── snapshots/
├── benches/
├── fuzz/
├── examples/
├── docs/
│   ├── architecture/
│   ├── decisions/
│   ├── engineering/
│   ├── quality/
│   ├── testing/
│   ├── performance/
│   ├── security/
│   └── operations/
├── scripts/
├── tools/
└── xtask/
```

### 5.2 Respect des conventions Cargo

Cargo reconnaît naturellement :

```text
src/lib.rs
src/main.rs
src/bin/
tests/
benches/
examples/
```

La charte ne doit pas inventer une structure contraire à Cargo. Les conventions Cargo sont le socle ; les dossiers projet viennent ensuite.

### 5.3 Rôle des dossiers

| Dossier | Rôle | Règle |
|---|---|---|
| `crates/` | crates du workspace | une crate = responsabilité explicite. |
| `tests/` | tests workspace transverses | pas de logique métier cachée. |
| `benches/` | benchmarks reproductibles | résultats interprétables. |
| `fuzz/` | harness fuzzing | surfaces binaires/protocolaires. |
| `examples/` | exemples compilables | pas de snippets morts. |
| `docs/` | décisions et guides | versionnés et maintenus. |
| `scripts/` | scripts simples | pas de logique critique. |
| `tools/` | outils internes | documentés, testés si critiques. |
| `xtask/` | automatisation Rust-native | privilégier pour tâches complexes. |

### 5.4 Politique `xtask`

Le pattern `xtask` est recommandé lorsque :

- un script devient complexe ;
- une tâche doit être portable ;
- une tâche doit manipuler le workspace ;
- une tâche doit être testable ;
- une tâche doit générer du code, des rapports ou des validations.

Exemples :

```bash
cargo xtask quality
cargo xtask audit
cargo xtask cleanup-report
cargo xtask dependency-graph
cargo xtask generate-contracts
cargo xtask recovery-test
```

---

## 6. Topologie de crates pour Andromeda

### 6.1 Principe

Andromeda ne doit pas devenir un monolithe Rust sous prétexte qu’il s’agit d’un moteur.

La bonne séparation est :

```text
Core minimal
Types stables
Catalog contractuel
Execution orchestration
Transaction kernel
Storage persistant
Protocol/RPC séparé
Security séparée
Optimizer/statistics séparés
Testing support isolé
```

### 6.2 Crates recommandées

| Crate | Responsabilité |
|---|---|
| `andromeda-core` | primitives bas niveau, clocks, ids, erreurs minimales, capabilities. |
| `andromeda-types` | types fondamentaux, newtypes, encodages, scalaires, invariants. |
| `andromeda-catalog` | objets catalogués, versions, descriptors, DefinitionBatch. |
| `andromeda-contract` | ProcedureContract, ContractHash, shapes, compatibilité. |
| `andromeda-srpl` | lexer, parser, AST, binder, diagnostics SRPL. |
| `andromeda-exec` | orchestration d’invocation, dispatch, lifecycle d’exécution. |
| `andromeda-tx` | Transaction Kernel, états, isolation, MVCC interface. |
| `andromeda-wal` | WalRecord, LogStream, checksums, replay primitives. |
| `andromeda-storage` | pages, extents, manifests, hot/cold stores. |
| `andromeda-buffer` | BufferPool, page pinning, eviction, dirty tracking. |
| `andromeda-rpc` | frames applicatives, payloads, ResultStream, erreurs protocole. |
| `andromeda-quic` | transport QUIC, sessions, streams, backpressure transport. |
| `andromeda-security` | IAM, certificats, permissions, policies, audit sécurité. |
| `andromeda-observability` | traces, metrics, structured diagnostics. |
| `andromeda-optimizer` | plans, costing, plan cache, decision traces. |
| `andromeda-stats` | histogrammes, cardinalité, StatsVersion. |
| `andromeda-cli` | outils ligne de commande. |
| `andromeda-testing` | fixtures, builders, crash harness, helpers de tests. |

### 6.3 Crates à éviter

Éviter les noms :

```text
common
utils
misc
helpers
shared
base
stuff
engine
```

Sauf si le périmètre est formellement défini.

Un nom vague crée une zone d’accumulation.

### 6.4 Dépendances directionnelles

Exemple de dépendances acceptables :

```text
types -> core
catalog -> types, core
contract -> catalog, types
srpl -> contract, catalog, types
tx -> wal, types, core
storage -> wal, types, core
exec -> srpl, tx, storage, catalog, contract
rpc -> contract, types
quic -> rpc
security -> catalog, types
optimizer -> catalog, stats, contract
```

À éviter :

```text
storage -> exec
wal -> storage complet
types -> catalog
core -> tout le monde
security -> quic spécifique si non nécessaire
optimizer -> exec runtime complet
testing-support -> production logic
```

### 6.5 Règle anti-cycle

Rust empêche les cycles Cargo directs, mais pas les cycles conceptuels.

Un cycle conceptuel existe si :

- deux crates ne peuvent plus être comprises séparément ;
- les types d’une crate sont écrits pour satisfaire l’autre ;
- une abstraction existe seulement pour contourner une dépendance inversée ;
- des traits sont placés dans `core` uniquement pour casser artificiellement le cycle.

Si un cycle conceptuel apparaît, créer une crate de contrat minimal ou inverser la dépendance.

---

## 7. Structure interne d’une crate

### 7.1 Gabarit général

```text
crates/andromeda-storage/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs
│   ├── error.rs
│   ├── page/
│   │   ├── mod.rs
│   │   ├── header.rs
│   │   ├── trailer.rs
│   │   ├── layout.rs
│   │   └── checksum.rs
│   ├── extent/
│   │   ├── mod.rs
│   │   ├── allocation.rs
│   │   └── descriptor.rs
│   ├── manifest/
│   │   ├── mod.rs
│   │   ├── descriptor.rs
│   │   ├── validation.rs
│   │   └── switch.rs
│   └── snapshot/
│       ├── mod.rs
│       ├── builder.rs
│       └── publisher.rs
└── tests/
    ├── page_format_tests.rs
    ├── manifest_tests.rs
    └── snapshot_publication_tests.rs
```

### 7.2 Rôle de `lib.rs`

`lib.rs` doit rester une surface d’intention.

Acceptable :

```rust
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod error;
pub mod page;
pub mod manifest;

pub use error::StorageError;
pub use page::{PageId, PageType};
pub use manifest::DatabaseManifest;
```

À éviter :

```rust
// 1200 lignes de logique métier, imports, tests, helpers, impls.
```

### 7.3 Rôle de `mod.rs`

`mod.rs` peut :

- déclarer les sous-modules ;
- exposer une API locale ;
- réexporter les types principaux ;
- contenir une documentation de module.

Il ne doit pas devenir un conteneur massif de logique.

### 7.4 Tests proches du code

Règle :

```text
Tests unitaires proches du module.
Tests d’intégration dans tests/.
Tests transverses au workspace dans tests/ racine.
```

---

## 8. Découpage des fichiers

### 8.1 Seuils de vigilance

Ces seuils ne sont pas des lois Rust. Ce sont des déclencheurs de revue.

| Élément | Seuil de vigilance | Action attendue |
|---|---:|---|
| Fichier `.rs` | 300–500 lignes | analyser split. |
| Fonction simple | 30–50 lignes | extraire étapes nommées. |
| Fonction complexe | 50–80 lignes | refactor prioritaire. |
| Module | 5–8 concepts distincts | sous-modules. |
| Enum | trop de variantes hétérogènes | séparer états/concepts. |
| Struct | trop de champs incohérents | value objects / sous-structures. |
| Impl block | mélange construction, validation, I/O | séparer responsabilités. |
| Test file | trop de scénarios mélangés | split par comportement. |

### 8.2 Signaux de fichier trop lourd

Un fichier doit être découpé si :

- il contient plusieurs domaines ;
- il contient plusieurs niveaux d’abstraction ;
- il mélange types, parsing, I/O, validation et tests ;
- il possède trop d’imports non liés ;
- il oblige à scroller longtemps pour comprendre les types principaux ;
- il contient plusieurs familles d’erreurs ;
- il contient des helpers sans lien direct ;
- les tests deviennent difficiles à localiser.

### 8.3 Méthode de split

Procédure recommandée :

```text
1. Identifier les responsabilités réelles.
2. Nommer les sous-domaines.
3. Extraire les types purs.
4. Extraire la validation.
5. Extraire les effets de bord.
6. Extraire les tests associés.
7. Réduire la visibilité.
8. Relancer fmt/check/clippy/test.
9. Relire les imports et réexports.
```

### 8.4 Split incorrect

À éviter :

```text
page1.rs
page2.rs
helpers.rs
misc.rs
common.rs
```

Un split par taille seulement déplace le problème.

### 8.5 Split correct

Préférer :

```text
page/header.rs
page/trailer.rs
page/layout.rs
page/checksum.rs
page/slot_directory.rs
```

Le nom du fichier doit refléter une responsabilité, pas une tranche arbitraire.

---

## 9. Politique de clean-up

### 9.1 Objectif

Le clean-up doit réduire :

- surface publique ;
- duplication ;
- dépendances ;
- warnings ;
- fichiers inutiles ;
- complexité locale ;
- couplage ;
- ambiguïtés ;
- tests morts ;
- documentation fausse.

### 9.2 Nettoyage minimal récurrent

À chaque cycle de nettoyage :

```bash
cargo fmt --all
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo tree -d
cargo tree -e features
```

Puis, selon contexte :

```bash
cargo machete
cargo +nightly udeps --workspace --all-targets
cargo deny check
cargo audit
cargo semver-checks
cargo nextest run --workspace --all-features
cargo llvm-cov nextest --workspace --all-features
```

### 9.3 Politique des warnings

Règle générale :

```text
Aucun warning permanent en branche principale.
```

Exceptions :

- warning de transition documenté ;
- dette technique avec issue ;
- incompatibilité temporaire liée à un outil ;
- code expérimental hors build principal.

### 9.4 TODO / FIXME

Un TODO doit contenir :

```text
TODO(#issue): action précise, raison, condition de suppression.
```

À éviter :

```rust
// TODO: clean this
// FIXME later
```

Préférer :

```rust
// TODO(#184): replace temporary page checksum with canonical Crc64 after WalRecord v0 is frozen.
```

### 9.5 Commentaires mensongers

Un commentaire faux est plus dangereux qu’une absence de commentaire.

Règle :

```text
Un commentaire obsolète doit être supprimé ou corrigé dans la même PR.
```

---

## 10. Détection des orphelins

### 10.1 Catégories

| Catégorie | Exemple | Détection |
|---|---|---|
| Fichier orphelin | `.rs` non relié à `mod` | script + grep + cargo check. |
| Module orphelin | module déclaré mais non utilisé | rustc/clippy. |
| Dépendance orpheline | crate dans Cargo.toml non utilisée | cargo-udeps, cargo-machete. |
| Feature orpheline | feature jamais activée | cargo-hack, CI matrix. |
| Test orphelin | test ignoré sans raison | grep `#[ignore]`. |
| Benchmark orphelin | bench jamais lancé | CI/perf manifest. |
| Exemple orphelin | example cassé | `cargo check --examples`. |
| Doc orpheline | doc contredit code | revue docs. |
| Script orphelin | script non appelé | docs + CI. |

### 10.2 Traitement

Chaque orphelin doit être :

1. supprimé ;
2. reconnecté ;
3. déplacé vers `docs/archive/` si historique nécessaire ;
4. converti en issue datée ;
5. explicitement marqué expérimental.

Pas de cinquième état silencieux.

### 10.3 Rapport de clean-up

Un rapport de clean-up utile contient :

```text
Date
Auteur
Scope
Fichiers supprimés
Modules déplacés
Crates supprimées
Dépendances supprimées
Features supprimées
Tests réactivés
Docs corrigées
Risques
Commandes exécutées
Résultat CI
```

---

## 11. Refactorisation profonde

### 11.1 Protocole général

Une refactorisation profonde doit suivre :

```text
1. Stabiliser le comportement observable.
2. Ajouter ou renforcer les tests.
3. Cartographier dépendances et responsabilités.
4. Réduire la visibilité.
5. Extraire les types/invariants.
6. Extraire les modules.
7. Extraire les crates si nécessaire.
8. Supprimer l’ancien chemin.
9. Relancer quality gates.
10. Documenter la décision.
```

### 11.2 Refactorisation interdite

Refuser une refactorisation si :

- elle change le comportement sans l’assumer ;
- elle supprime des tests au lieu de les réparer ;
- elle introduit une abstraction plus floue ;
- elle agrandit la surface publique ;
- elle rend les erreurs moins typées ;
- elle cache un coût ;
- elle mélange clean-up et feature majeure sans séparation.

### 11.3 Refactorisation acceptable sans changement fonctionnel

Exemples :

- renommage clair ;
- extraction d’une fonction pure ;
- réduction de visibilité ;
- déplacement de tests ;
- split de module ;
- suppression d’un wrapper inutile ;
- remplacement d’un alias par newtype ;
- séparation d’une erreur métier et technique ;
- suppression de duplication réelle.

### 11.4 Refactorisation risquée

À traiter avec review dédiée :

- modification de lifetimes ;
- modification d’ownership ;
- modification de concurrence ;
- remplacement de structure de données ;
- changement d’erreur publique ;
- changement de feature ;
- changement d’API crate ;
- introduction d’`unsafe`;
- changement de layout binaire ;
- changement d’ordre d’exécution ;
- changement de sérialisation.

### 11.5 Refactorisation par strangler pattern

Pour les modules très gros :

```text
ancien module
  -> nouvelle interface minimale
  -> nouveau module propre
  -> migration des appels
  -> tests de compatibilité
  -> suppression ancien module
```

Ne pas tout réécrire en une seule PR si le risque est élevé.

---

## 12. Consolidation des responsabilités

### 12.1 Quand consolider

Consolider lorsque plusieurs éléments expriment le même concept :

- mêmes validations ;
- mêmes conversions ;
- mêmes erreurs ;
- mêmes accès I/O ;
- mêmes builders de tests ;
- mêmes règles de nommage ;
- mêmes stratégies de retry ;
- mêmes encodages ;
- mêmes calculs de hash.

### 12.2 Quand ne pas consolider

Ne pas consolider si :

- les concepts se ressemblent mais évoluent différemment ;
- la duplication est temporaire et clarifie le code ;
- l’abstraction n’a pas de nom stable ;
- le coût générique dépasse le gain ;
- le trait créé n’a qu’une implémentation artificielle ;
- la consolidation ajoute une dépendance entre domaines.

### 12.3 Règle de nommage

Une consolidation correcte doit produire un nom plus fort que `Helper`.

Exemples :

| Mauvais | Meilleur |
|---|---|
| `StorageHelper` | `PageChecksumValidator` |
| `ContractUtil` | `ContractHashCanonicalizer` |
| `TestUtils` | `WalCrashScenarioBuilder` |
| `CommonError` | `CatalogValidationError` |
| `DataHelper` | `StructuredObjectEncoder` |

---

## 13. Gestion des dépendances Cargo

### 13.1 Principe

Une dépendance est une dette contrôlée.

Avant d’ajouter une dépendance :

```text
Pourquoi ?
Usage exact ?
Alternatives std ?
Surface transitive ?
Maintenance ?
Licence ?
Sécurité ?
Impact binaire ?
Impact compilation ?
Features activées ?
Usage dans chemin critique ?
```

### 13.2 Dépendances interdites ou suspectes

Refuser ou challenger :

- crates abandonnées ;
- crates avec dépendances transitives énormes pour un usage minime ;
- crates qui activent beaucoup de default features ;
- crates qui imposent runtime global ;
- crates non auditées pour chemin C5 ;
- crates de macro complexes sans gain fort ;
- crates de crypto non reconnues ;
- crates de sérialisation non maîtrisées pour formats critiques.

### 13.3 cargo-deny

Le dépôt doit utiliser `cargo-deny` pour :

- advisories ;
- licences ;
- sources ;
- duplications ;
- bannissement de crates ;
- contrôle des versions multiples.

### 13.4 RustSec / cargo-audit

`cargo audit` doit être exécuté en CI.

Politique :

| Gravité | Réponse |
|---|---|
| Critical | blocage immédiat. |
| High | blocage sauf exception formelle. |
| Medium | issue obligatoire et délai. |
| Low | suivi normal. |
| Unmaintained | revue selon criticité. |

### 13.5 cargo tree

Commandes utiles :

```bash
cargo tree
cargo tree -d
cargo tree -e features
cargo tree -i crate_name
```

Usage :

- comprendre pourquoi une dépendance est présente ;
- détecter les versions dupliquées ;
- identifier les features activées ;
- réduire la surface transitive.

---

## 14. Politique des features

### 14.1 Règle générale

Les features doivent être :

```text
additives
documentées
testées
nommées clairement
sans conflit implicite
```

### 14.2 Interdiction des features non testées

Une feature non couverte par CI est une hypothèse.

Utiliser :

```bash
cargo hack check --workspace --feature-powerset --no-dev-deps
cargo hack test --workspace --each-feature
```

À adapter selon coût.

### 14.3 Default features

Les default features doivent rester minimales.

À éviter :

```toml
default = ["all", "tokio/full", "serde", "gpu", "unstable"]
```

Préférer :

```toml
default = ["std"]
```

ou pas de default si la crate est bas niveau.

### 14.4 Features mutuellement exclusives

Cargo n’est pas naturellement conçu pour des features exclusives.

Si des features sont réellement exclusives :

- préférer crates séparées ;
- ou valider par `compile_error!`;
- ou documenter explicitement.

Exemple :

```rust
#[cfg(all(feature = "native-tls", feature = "rustls"))]
compile_error!("features `native-tls` and `rustls` are mutually exclusive");
```

---

## 15. Lints workspace

### 15.1 Centralisation

Déclarer les lints au niveau workspace.

Exemple :

```toml
[workspace.lints.rust]
unsafe_code = "forbid"
unused_crate_dependencies = "warn"
unreachable_pub = "warn"
missing_docs = "warn"

[workspace.lints.clippy]
all = "warn"
pedantic = "warn"
nursery = "warn"
unwrap_used = "warn"
expect_used = "warn"
panic = "warn"
todo = "warn"
dbg_macro = "deny"
print_stdout = "warn"
print_stderr = "warn"
large_enum_variant = "warn"
large_stack_arrays = "warn"
```

Dans chaque crate :

```toml
[lints]
workspace = true
```

### 15.2 Exceptions

Certaines crates peuvent autoriser `unsafe` :

- `andromeda-storage`;
- `andromeda-wal`;
- `andromeda-buffer`;
- crates FFI/hardware.

Mais l’exception doit être locale et documentée.

```rust
#![deny(unsafe_op_in_unsafe_fn)]
```

### 15.3 `unwrap` et `expect`

Politique recommandée :

| Zone | `unwrap` / `expect` |
|---|---|
| tests | autorisé avec modération. |
| build scripts | possible si message clair. |
| prototype expérimental | toléré hors main. |
| serveur critique | interdit. |
| parsing réseau | interdit. |
| WAL/recovery | interdit. |
| storage C5 | interdit sauf preuve impossible autrement. |

Préférer :

```rust
value.ok_or(Error::MissingField { field: "page_id" })?
```

### 15.4 `panic!`

Un panic serveur est une perte de contrôle.

Règle :

```text
panic! interdit dans les chemins critiques runtime.
```

Acceptable :

- tests ;
- invariants impossibles dans code généré vérifié ;
- outils offline ;
- crash volontaire en bootstrap si configuration invalide et aucun état durable n’est touché.

---

## 16. Gestion des erreurs

### 16.1 Erreurs typées par domaine

Préférer :

```rust
#[derive(Debug, thiserror::Error)]
pub enum WalError {
    #[error("invalid WAL magic: expected {expected:#x}, actual {actual:#x}")]
    InvalidMagic { expected: u32, actual: u32 },

    #[error("truncated WAL record at LSN {lsn}")]
    TruncatedRecord { lsn: Lsn },

    #[error("checksum mismatch at LSN {lsn}")]
    ChecksumMismatch { lsn: Lsn },
}
```

À éviter :

```rust
Err("wal error".into())
```

### 16.2 `thiserror` vs `anyhow`

| Usage | Recommandation |
|---|---|
| crates bibliothèque | `thiserror` ou erreurs manuelles typées. |
| binaire CLI | `anyhow` possible. |
| `xtask` | `anyhow` acceptable. |
| tests | `anyhow` acceptable. |
| API publique moteur | erreurs typées. |
| protocole RPC | erreurs codées et sérialisables. |

### 16.3 Erreurs sérialisables

Pour les frontières RPC, stockage ou contrat, une erreur doit avoir :

```text
code stable
message humain
famille
source éventuelle
corrélation trace
sévérité
retriable ou non
```

### 16.4 Erreurs et observabilité

Une erreur critique doit produire :

- trace structurée ;
- contexte minimal ;
- identifiant de corrélation ;
- pas de fuite de secret ;
- classification stable.

---

## 17. `unsafe` Rust

### 17.1 Doctrine

Rust permet d’écrire du code système. `unsafe` n’est pas interdit par principe dans un moteur de base de données, mais il doit être rare, encapsulé et prouvé localement.

Règle :

```text
unsafe est un détail d’implémentation local derrière une API safe.
```

### 17.2 Zones où unsafe peut être légitime

- layouts binaires ;
- pages alignées ;
- buffers non initialisés ;
- I/O direct ;
- FFI ;
- intrinsics CPU ;
- SIMD ;
- mmap ;
- structures lock-free ;
- allocateurs spécialisés ;
- conversion contrôlée de bytes vers structures.

### 17.3 Obligations d’un bloc unsafe

Chaque bloc unsafe doit avoir :

```rust
// SAFETY:
// - invariant 1
// - invariant 2
// - pourquoi le pointeur est valide
// - pourquoi l’alignement est correct
// - pourquoi la durée de vie est respectée
unsafe {
    ...
}
```

### 17.4 Interdictions

Interdit :

- `unsafe` diffus dans code métier ;
- `unsafe` sans commentaire `SAFETY`;
- `unsafe` dans PR sans reviewer compétent ;
- `unsafe` pour contourner le borrow checker par confort ;
- `transmute` sans preuve forte ;
- aliasing mutable non justifié ;
- lecture de mémoire non initialisée ;
- dépendance à l’endian natif dans format persistant.

### 17.5 Validation unsafe

Selon criticité :

```bash
cargo miri test
cargo fuzz run target_name
cargo geiger
cargo llvm-cov
```

Pour concurrence :

```rust
loom
```

Pour preuve ciblée :

```text
Kani
```

### 17.6 Unsafe et Andromeda C5

Dans les composants critiques :

- WAL ;
- recovery ;
- pages ;
- buffer pool ;
- transaction kernel ;
- RPC framing ;

tout unsafe doit être traité comme une décision d’architecture, pas comme un détail de code.

---

## 18. Concurrence et async

### 18.1 Principes

Concurrence Rust correcte :

```text
ownership clair
partage minimal
verrous bornés
pas de lock global inutile
pas de blocage dans async
annulation contrôlée
shutdown explicite
observabilité des tâches
```

### 18.2 Anti-patterns

À éviter :

- `Arc<Mutex<Everything>>`;
- verrous conservés pendant `.await`;
- tâches spawnées sans supervision ;
- channels sans backpressure ;
- retries infinis ;
- timeouts non traçables ;
- cancellation ignorée ;
- blocking I/O dans runtime async ;
- mélange sync/async non documenté.

### 18.3 Règles async

Si usage de Tokio ou runtime équivalent :

- pas de `std::thread::sleep` dans async ;
- pas de lock sync gardé pendant `.await`;
- utiliser timeouts ;
- propager cancellation ;
- instrumenter tasks et spans ;
- limiter les buffers ;
- backpressure obligatoire pour flux réseau.

### 18.4 Concurrence déterministe en test

Pour les composants sensibles :

- tester avec `loom`;
- réduire les interleavings ;
- isoler les primitives ;
- ne pas tester uniquement le cas heureux.

### 18.5 Shutdown

Chaque service doit avoir :

```text
start
ready
drain
stop accepting
flush
shutdown
join
report
```

Un arrêt propre est une fonctionnalité.

---

## 19. Performance Rust

### 19.1 Règle cardinale

```text
Pas d’optimisation sans mesure.
Pas de mesure sans scénario.
Pas de scénario sans hypothèse.
```

### 19.2 Mesures minimales

Selon composant :

- temps CPU ;
- allocations ;
- copies ;
- tailles de buffers ;
- latence p50/p95/p99 ;
- débit ;
- contention ;
- temps de compilation ;
- taille binaire ;
- bytes WAL ;
- bytes réseau ;
- writes NVMe ;
- cache hits/misses.

### 19.3 Allocation

Pratiques :

- préallouer avec `Vec::with_capacity` quand cardinalité connue ;
- éviter `String` quand `&str` suffit ;
- éviter `clone` profond non justifié ;
- passer par slices ;
- préférer itérateurs lisibles mais mesurer ;
- utiliser `Bytes`/buffers partagés uniquement si justifié ;
- éviter allocations dans hot path.

### 19.4 Clone

Tout `clone` dans chemin critique doit être justifié.

Questions :

```text
Clone de quoi ?
Taille typique ?
Taille maximale ?
Fréquence ?
Alternative par emprunt ?
Alternative par Arc ?
Coût acceptable ?
```

### 19.5 Monomorphisation

Rust générique peut produire du code très rapide, mais aussi augmenter :

- taille binaire ;
- temps de compilation ;
- instruction cache pressure.

Règle :

```text
Généricité pour invariants et performance prouvée, pas par réflexe.
```

### 19.6 Profils Cargo

Exemple :

```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
debug = "line-tables-only"
strip = "symbols"
panic = "abort"

[profile.profiling]
inherits = "release"
debug = true
strip = "none"

[profile.dev]
debug = true
incremental = true

[profile.ci]
inherits = "dev"
debug = 1
incremental = false
```

Attention : `panic = "abort"` doit être décidé au niveau binaire et selon stratégie opérationnelle.

### 19.7 cargo timings

Utiliser :

```bash
cargo build --timings
```

Objectif :

- détecter crates lentes ;
- réduire features inutiles ;
- identifier macros coûteuses ;
- réduire dépendances lourdes ;
- choisir découpage de crates.

### 19.8 Benchmarks

Utiliser `criterion` ou un outil stable équivalent pour microbenchmarks.

Règles :

- benchmark reproductible ;
- données réalistes ;
- warm-up ;
- seuils de régression ;
- ne pas confondre microbench et performance système.

---

## 20. Formats binaires et données persistantes

### 20.1 Règle générale

Un format persistant est une API publique.

Il doit définir :

```text
magic
version
endianness
alignment
size
checksum
hash
compatibilité
migration
validation
erreurs
tests de corruption
```

### 20.2 Endianness

Ne jamais dépendre de l’endian natif pour un format persistant.

Décision recommandée :

```text
little-endian canonique
```

ou autre, mais documenté.

### 20.3 `repr`

Utiliser :

```rust
#[repr(transparent)]
```

pour newtypes ABI-safe.

Utiliser `#[repr(C)]` uniquement si nécessaire et documenté.

Ne jamais supposer qu’une struct Rust normale est stable en mémoire.

### 20.4 Sérialisation critique

Pour WAL, pages, manifests, protocol frames :

- pas de sérialisation magique non auditée ;
- version de format explicite ;
- test golden ;
- fuzzing ;
- rejet des payloads invalides ;
- validation longueur avant lecture ;
- quotas.

---

## 21. Tests

### 21.1 Pyramide de tests

| Niveau | Objectif |
|---|---|
| Unit tests | logique locale. |
| Integration tests | interactions de crates. |
| Contract tests | API/contrats stables. |
| Property tests | invariants sous données générées. |
| Fuzz tests | parsing, protocole, formats binaires. |
| Concurrency tests | interleavings, races, cancellation. |
| Crash/recovery tests | durabilité et reprise. |
| Bench tests | non-régression performance. |
| Golden tests | formats, diagnostics, snapshots. |

### 21.2 Tests unitaires

À placer proche du module.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_page_magic() {
        // ...
    }
}
```

### 21.3 Tests d’intégration

À placer dans `tests/`.

Ils doivent tester :

- API publique de crate ;
- comportement observable ;
- interactions entre composants.

### 21.4 Property testing

Utiliser `proptest` pour :

- parsers ;
- encode/decode ;
- checksums ;
- invariants de structure ;
- canonicalisation ;
- idempotence ;
- roundtrip.

Exemples d’invariants :

```text
decode(encode(x)) = x
canonicalize(canonicalize(x)) = canonicalize(x)
valid manifest -> stable hash
invalid length -> rejected, never panic
```

### 21.5 Fuzzing

Cibles prioritaires Andromeda :

- RPC FrameHeader ;
- StructuredObject payload ;
- WAL records ;
- page headers/trailers ;
- manifests ;
- SRPL lexer/parser ;
- contract hash canonicalization.

### 21.6 Crash/recovery tests

Obligatoire pour moteur transactionnel.

Scénarios :

| Scénario | Attendu |
|---|---|
| crash avant WAL flush | mutation invisible. |
| crash après WAL flush | mutation visible après recovery. |
| WAL tronqué | replay jusqu’au dernier record valide. |
| manifest corrompu | fallback manifest précédent. |
| page torn write | détection via checksum/hash. |
| transaction incomplète | rollback/undo. |
| checkpoint incomplet | non publié. |
| snapshot tmp incomplet | non publié. |

### 21.7 Tests ignorés

Un test ignoré doit expliquer pourquoi :

```rust
#[ignore = "requires deterministic crash harness, tracked by #241"]
```

Aucun `#[ignore]` sans issue.

---

## 22. Quality gates

### 22.1 Gate local minimale

Avant commit :

```bash
cargo fmt --all
cargo check --workspace --all-targets
cargo test --workspace
```

### 22.2 Gate PR standard

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --all-features --no-deps
cargo tree -d
cargo deny check
cargo audit
```

### 22.3 Gate avancée

```bash
cargo nextest run --workspace --all-features
cargo llvm-cov nextest --workspace --all-features
cargo hack check --workspace --each-feature
cargo machete
cargo +nightly udeps --workspace --all-targets
cargo semver-checks
```

### 22.4 Gate critique C5

Pour WAL/storage/transaction/recovery/security/protocol :

```bash
cargo nextest run --workspace --all-features
cargo fuzz run <target>
cargo miri test -p <crate>
cargo geiger
```

Selon disponibilité :

```text
Loom tests
Kani proofs ciblées
Sanitizers
Crash harness
```

### 22.5 Politique de blocage

| Échec | Décision |
|---|---|
| fmt | bloque. |
| check | bloque. |
| clippy -D warnings | bloque. |
| tests | bloque. |
| audit high/critical | bloque. |
| deny license/source | bloque. |
| unsafe non documenté | bloque sur C5. |
| coverage en baisse critique | bloque selon politique. |
| benchmark régression | bloque si chemin critique. |

---

## 23. Documentation

### 23.1 Niveaux documentaires

| Niveau | Fichier |
|---|---|
| vision | `README.md`, `ARCHITECTURE.md`. |
| contribution | `CONTRIBUTING.md`. |
| qualité | `QUALITY.md`. |
| sécurité | `SECURITY.md`. |
| décisions | `docs/decisions/DEC-xxx.md`. |
| opérations | `docs/operations/`. |
| performance | `docs/performance/`. |
| testing | `docs/testing/`. |
| API rustdoc | docs générées par `cargo doc`. |

### 23.2 Décisions d’architecture

Chaque décision importante doit avoir :

```text
Contexte
Décision
Alternatives
Conséquences
Risques
Critères de révision
Date
Statut
```

### 23.3 Documentation module

Pour une crate critique :

```rust
//! Storage primitives for Andromeda.
//!
//! This crate owns page layout, manifests and snapshot publication.
//! It does not own transaction state or WAL durability decisions.
//!
//! Invariants:
//! - persistent formats are versioned;
//! - no native-endian persistence;
//! - invalid bytes must return errors, never panic.
```

### 23.4 Documentation des invariants

Un invariant doit être écrit près du code qui le protège.

Exemple :

```rust
/// A page payload is valid only if its trailer checksum matches the
/// canonical byte representation of the header and payload.
```

---

## 24. Review de code

### 24.1 Types de review

| Type | Quand |
|---|---|
| Review standard | changement local non critique. |
| Review architecture | frontières de crates/modules. |
| Review performance | hot path, allocations, profils. |
| Review sécurité | IAM, protocole, secrets. |
| Review unsafe | tout nouveau unsafe. |
| Review recovery | WAL, commit, crash, manifest. |
| Review API | public API, contrats, erreurs. |

### 24.2 Checklist PR

Une PR doit répondre :

```text
Quel problème est résolu ?
Quel comportement change ?
Quels tests prouvent le comportement ?
Quelle surface publique change ?
Quelles dépendances sont ajoutées ?
Y a-t-il unsafe ?
Y a-t-il impact performance ?
Y a-t-il impact format/protocole ?
Y a-t-il impact recovery ?
Y a-t-il migration nécessaire ?
Comment revenir en arrière ?
```

### 24.3 PR de refactorisation

Une PR de refactorisation doit être claire :

```text
No functional change intended.
```

Si le comportement change, ce n’est plus une pure refactorisation.

### 24.4 Taille des PR

Recommandation :

| Type | Taille |
|---|---|
| clean-up simple | petite. |
| rename mécanique | isolée. |
| split module | isolée. |
| changement comportemental | séparé du clean-up. |
| changement API | review dédiée. |
| unsafe | petite et très documentée. |

---

## 25. Observabilité du code

### 25.1 Logs vs traces

Préférer `tracing` structuré à des logs libres.

À éviter :

```rust
println!("error");
dbg!(value);
```

Préférer :

```rust
tracing::warn!(
    lsn = %lsn,
    record_type = ?record_type,
    "invalid WAL record checksum"
);
```

### 25.2 Champs stables

Les traces doivent utiliser des noms stables :

```text
trace_id
invocation_id
tx_id
lsn
page_id
catalog_version
contract_hash
procedure_id
plan_id
error_code
```

### 25.3 Pas de secrets

Interdiction :

- token ;
- clé privée ;
- certificat complet ;
- payload métier sensible ;
- mot de passe ;
- secret de connexion.

### 25.4 Observabilité de refactorisation

Après refactorisation, conserver ou améliorer :

- traces existantes ;
- compteurs ;
- métriques ;
- diagnostics ;
- corrélation.

---

## 26. Sécurité

### 26.1 Dépendances

Toute dépendance introduite dans une zone critique doit être auditée.

Questions :

```text
Qui maintient ?
Quelle licence ?
Combien de dépendances transitives ?
Y a-t-il unsafe ?
Y a-t-il advisories ?
Est-elle nécessaire dans le hot path ?
Est-elle nécessaire dans C5 ?
```

### 26.2 Secrets

Aucun secret dans :

- tests ;
- fixtures ;
- logs ;
- snapshots ;
- exemples ;
- documentation ;
- panic messages ;
- erreurs retournées au client.

### 26.3 Parsing hostile

Tout input externe est hostile :

- réseau ;
- fichier ;
- WAL après crash ;
- page storage ;
- manifest ;
- StructuredObject ;
- SRPL source ;
- contrat client.

Règle :

```text
Invalid input => typed error, never panic.
```

### 26.4 Resource exhaustion

Défenses :

- limites de taille ;
- quotas ;
- timeouts ;
- backpressure ;
- validation avant allocation ;
- streaming ;
- refus explicite.

---

## 27. Application à Andromeda

### 27.1 Criticité par composant

| Composant | Criticité | Exigence |
|---|---:|---|
| WAL | C5 | tests crash, fuzz, no panic. |
| Recovery | C5 | déterministe, rapports, corruption tests. |
| Transaction Kernel | C5 | state machine testée. |
| Storage pages | C5 | formats versionnés, checksum, fuzz. |
| RPC framing | C4/C5 | hostile input, quotas. |
| Security IAM | C5 | audit, deny by default. |
| Catalog | C5 | versioning, WAL, compatibilité. |
| SRPL parser | C3/C4 | diagnostics, fuzz. |
| Optimizer | C3 | decision trace, fallback. |
| Statistics | C2/C3 | versioning, validation. |
| GPU analytics | C1/C2 | jamais commit path. |
| CLI | C1/C2 | ergonomie, pas critique runtime. |

### 27.2 Interdictions Andromeda

```text
Pas de SQL ad hoc applicatif.
Pas de procédure qui accède au réseau externe.
Pas de procédure qui accède au filesystem externe.
Pas de GPU dans le commit path.
Pas de WAL sans checksum.
Pas de format persistant sans version.
Pas de plan actif sans CatalogVersion/StatsVersion/ContractHash.
Pas de panic dans recovery.
Pas d’erreur protocolaire non typée.
Pas de dépendance critique non auditée.
```

### 27.3 Noms alignés projet

Préférer :

```text
Procedure
Invocation
Procedure Store
Procedure Plan
Procedure Plan Cache
CatalogVersion
StatsVersion
ContractHash
ResultStream
StructuredObject
DefinitionBatch
Map
```

Éviter comme vocabulaire natif :

```text
Query
Query Store
View
Migration script
Dynamic SQL
```

---

## 28. Plan de clean-up massif

### 28.1 Phase 1 — Inventaire

Produire :

```bash
cargo metadata --format-version 1 > target/metadata.json
cargo tree > target/tree.txt
cargo tree -e features > target/features.txt
cargo tree -d > target/duplicates.txt
```

Lister :

- crates ;
- modules ;
- fichiers ;
- features ;
- dépendances ;
- tests ;
- exemples ;
- benches ;
- scripts ;
- docs.

### 28.2 Phase 2 — Classification

Classer chaque élément :

| Statut | Sens |
|---|---|
| Keep | utile et sain. |
| Refactor | utile mais mal structuré. |
| Split | trop gros ou mélangé. |
| Merge | duplication conceptuelle. |
| Delete | mort ou inutile. |
| Archive | historique utile mais hors runtime. |
| Experimental | conservé mais isolé. |

### 28.3 Phase 3 — Suppression simple

Supprimer d’abord :

- imports inutiles ;
- fichiers non référencés ;
- tests cassés sans valeur ;
- exemples cassés ;
- dépendances évidentes ;
- scripts historiques ;
- docs fausses.

### 28.4 Phase 4 — Réduction de visibilité

Passer de :

```rust
pub
```

à :

```rust
pub(crate)
pub(super)
private
```

lorsque possible.

### 28.5 Phase 5 — Split structurel

Découper les fichiers lourds par responsabilité.

### 28.6 Phase 6 — Consolidation

Fusionner les concepts réellement identiques.

### 28.7 Phase 7 — Quality gates

Relancer gates complets.

### 28.8 Phase 8 — Documentation

Documenter :

- ce qui a été supprimé ;
- ce qui a été conservé ;
- les dettes restantes ;
- les prochaines extractions.

---

## 29. Plan de refactorisation d’une crate

### 29.1 Exemple : crate storage

Étapes :

```text
1. Identifier API publique réelle.
2. Réduire lib.rs.
3. Extraire page/.
4. Extraire manifest/.
5. Extraire snapshot/.
6. Extraire errors par domaine.
7. Ajouter tests de roundtrip.
8. Ajouter fuzz sur bytes invalides.
9. Documenter formats persistants.
10. Ajouter benchmarks ciblés.
```

### 29.2 Exemple : crate rpc

Étapes :

```text
1. Séparer transport et protocole.
2. Garder QUIC hors frame parsing.
3. Définir FrameHeader canonique.
4. Valider PayloadLength avant allocation.
5. Ajouter fuzz sur frames.
6. Ajouter tests backpressure.
7. Typiser les erreurs protocole.
8. Documenter compatibilité.
```

### 29.3 Exemple : crate srpl

Étapes :

```text
1. Lexer sans effet de bord.
2. Parser pur.
3. AST séparé.
4. Diagnostics séparés.
5. Binder séparé.
6. IR séparé.
7. Tests golden de diagnostics.
8. Fuzz lexer/parser.
9. Pas de dépendance sur execution runtime.
```

---

## 30. Modèles de fichiers

### 30.1 Cargo.toml workspace

```toml
[workspace]
resolver = "3"
members = [
    "crates/andromeda-core",
    "crates/andromeda-types",
    "crates/andromeda-catalog",
    "crates/andromeda-contract",
    "crates/andromeda-srpl",
    "crates/andromeda-exec",
    "crates/andromeda-tx",
    "crates/andromeda-wal",
    "crates/andromeda-storage",
    "crates/andromeda-buffer",
    "crates/andromeda-rpc",
    "crates/andromeda-quic",
    "crates/andromeda-security",
    "crates/andromeda-observability",
    "crates/andromeda-optimizer",
    "crates/andromeda-stats",
    "crates/andromeda-cli",
    "crates/andromeda-testing",
    "xtask",
]

[workspace.package]
edition = "2024"
rust-version = "1.95"
license = "Proprietary"
repository = "https://github.com/AriusII/Andromeda"

[workspace.lints.rust]
unsafe_code = "forbid"
unused_crate_dependencies = "warn"
unreachable_pub = "warn"
missing_docs = "warn"

[workspace.lints.clippy]
all = "warn"
pedantic = "warn"
nursery = "warn"
dbg_macro = "deny"
todo = "warn"
unwrap_used = "warn"
expect_used = "warn"
panic = "warn"
```

### 30.2 rustfmt.toml

```toml
edition = "2024"
style_edition = "2024"
newline_style = "Unix"
use_field_init_shorthand = true
use_try_shorthand = true
```

### 30.3 deny.toml minimal

```toml
[advisories]
vulnerability = "deny"
unmaintained = "warn"
yanked = "deny"
notice = "warn"

[licenses]
unlicensed = "deny"
allow = [
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-DFS-2016",
]

[bans]
multiple-versions = "warn"
wildcards = "deny"

[sources]
unknown-registry = "deny"
unknown-git = "warn"
```

### 30.4 Template DEC

```markdown
# DEC-XXX — Titre

## Statut

Proposed / Accepted / Rejected / Superseded

## Contexte

## Décision

## Alternatives étudiées

## Conséquences positives

## Conséquences négatives

## Impact Rust

## Impact tests

## Impact performance

## Impact sécurité

## Critères de révision
```

---

## 31. Definition of Done

Une tâche est terminée seulement si :

- le code compile ;
- `cargo fmt --all --check` passe ;
- `cargo clippy ... -D warnings` passe ;
- les tests pertinents passent ;
- les erreurs sont typées ;
- la visibilité est minimale ;
- les dépendances ajoutées sont justifiées ;
- les features sont documentées ;
- les docs nécessaires sont mises à jour ;
- les traces utiles sont présentes ;
- la performance est mesurée si impactée ;
- aucun TODO silencieux n’est ajouté ;
- aucune dette non suivie n’est introduite ;
- les reviewers adaptés ont validé.

Pour C5, ajouter :

- crash/recovery tests ;
- fuzz/property tests selon surface ;
- audit unsafe si concerné ;
- rapport de compatibilité format/protocole ;
- rollback plan.

---

## 32. Checklist clean-up

```text
[ ] Aucun fichier .rs non relié à l’arbre de modules.
[ ] Aucun module mort.
[ ] Aucune dépendance inutilisée.
[ ] Aucune feature non documentée.
[ ] Aucun test ignoré sans raison.
[ ] Aucun exemple cassé.
[ ] Aucun benchmark obsolète.
[ ] Aucun warning permanent.
[ ] Aucun TODO sans issue.
[ ] Aucun pub injustifié.
[ ] Aucun helper fourre-tout.
[ ] Aucune doc mensongère.
[ ] cargo fmt passe.
[ ] cargo check passe.
[ ] cargo clippy passe.
[ ] cargo test passe.
[ ] cargo deny/audit passe.
```

---

## 33. Checklist refactorisation

```text
[ ] Le comportement attendu est couvert par tests.
[ ] La PR indique si le comportement change ou non.
[ ] Les responsabilités avant/après sont décrites.
[ ] Les anciens chemins sont supprimés.
[ ] La visibilité est réduite.
[ ] Les noms sont plus précis.
[ ] Les erreurs restent typées.
[ ] Les performances ne régressent pas sans justification.
[ ] Les dépendances ne s’élargissent pas inutilement.
[ ] La documentation est mise à jour.
[ ] Les quality gates passent.
```

---

## 34. Checklist unsafe

```text
[ ] L’unsafe est nécessaire.
[ ] Une alternative safe a été évaluée.
[ ] Le bloc unsafe est minimal.
[ ] Le commentaire SAFETY est présent.
[ ] Les invariants sont testés.
[ ] Les entrées invalides sont rejetées.
[ ] Miri/fuzz/property tests ont été considérés.
[ ] Un reviewer compétent a validé.
[ ] L’API exposée reste safe.
```

---

## 35. Checklist performance

```text
[ ] Hypothèse écrite.
[ ] Benchmark ou mesure disponible.
[ ] Mesure avant.
[ ] Mesure après.
[ ] Impact allocation mesuré.
[ ] Impact taille binaire considéré.
[ ] Impact compilation considéré.
[ ] Complexité ajoutée acceptable.
[ ] Fallback ou rollback possible.
```

---

## 36. Anti-patterns Rust à surveiller

| Anti-pattern | Risque |
|---|---|
| `Arc<Mutex<AppState>>` global | contention, couplage. |
| `String` partout | allocations inutiles. |
| `clone()` par confort | coût caché. |
| `pub` partout | API incontrôlée. |
| crate `utils` | absence de frontières. |
| `unwrap()` runtime | panic. |
| `Box<dyn Error>` public | perte de typage. |
| features non testées | comportement fantôme. |
| macros excessives | compilation lente, debug difficile. |
| async sans backpressure | explosion mémoire. |
| lock pendant await | deadlock/latence. |
| unsafe non localisé | risque mémoire. |
| format binaire implicite | incompatibilité/recovery fragile. |

---

## 37. Vocabulaire recommandé pour demandes de clean-up

Pour formuler proprement une demande de nettoyage :

```text
Objectif :
Réduire la dette structurelle du workspace Rust.

Périmètre :
Crates, modules, fichiers, dépendances, features, tests, benches, examples, docs, scripts.

Actions attendues :
- détecter et supprimer le code mort ;
- détecter les éléments orphelins ;
- réduire les fichiers trop lourds ;
- splitter les modules par responsabilité ;
- consolider les duplications réelles ;
- réduire la visibilité publique ;
- renforcer les types métier ;
- supprimer les dépendances inutiles ;
- clarifier les erreurs ;
- renforcer les tests ;
- documenter les décisions.

Critères de succès :
- quality gates verts ;
- architecture plus lisible ;
- dépendances réduites ;
- tests maintenus ou renforcés ;
- aucune régression comportementale non déclarée.
```

---

## 38. Formulation type pour issue de refactorisation

```markdown
# Refactor — [crate/module] — [objectif]

## Contexte

## Problème actuel

- fichier trop long
- responsabilités mélangées
- dépendances trop larges
- visibilité excessive
- tests difficiles à maintenir

## Objectif

## Hors périmètre

## Plan proposé

1.
2.
3.

## Risques

## Tests attendus

## Quality gates

## Definition of Done
```

---

## 39. Politique d’évolution

### 39.1 Révision de la charte

Cette charte doit être revue lorsque :

- Rust change d’édition ;
- la MSRV change ;
- les quality gates évoluent ;
- une nouvelle classe de crate apparaît ;
- un composant devient C5 ;
- une pratique s’avère trop coûteuse ;
- un outil est abandonné ;
- une dette récurrente apparaît.

### 39.2 Versionnement

Format recommandé :

```text
QUALITY_RUST_STANDARD.md
version: 2.x
date
owner
status
```

### 39.3 Compatibilité avec Andromeda

Les règles générales peuvent être assouplies pour expérimentation, mais les composants C5 doivent suivre la version stricte.

---

## 40. Sources et références utiles

### 40.1 Sources Rust officielles

- Rust Blog — Announcing Rust 1.95.0 : https://blog.rust-lang.org/2026/04/16/Rust-1.95.0/
- Rust Edition Guide — Rust 2024 : https://doc.rust-lang.org/edition-guide/rust-2024/
- Cargo Book — Workspaces : https://doc.rust-lang.org/cargo/reference/workspaces.html
- Cargo Book — Package rust-version : https://doc.rust-lang.org/cargo/reference/rust-version.html
- Cargo Book — Features : https://doc.rust-lang.org/cargo/reference/features.html
- Cargo Book — Profiles : https://doc.rust-lang.org/cargo/reference/profiles.html
- Cargo command — cargo check : https://doc.rust-lang.org/cargo/commands/cargo-check.html
- Cargo command — cargo fix : https://doc.rust-lang.org/cargo/commands/cargo-fix.html
- Cargo command — cargo metadata : https://doc.rust-lang.org/cargo/commands/cargo-metadata.html
- Cargo command — cargo tree : https://doc.rust-lang.org/cargo/commands/cargo-tree.html
- Rustfmt style edition 2024 : https://doc.rust-lang.org/edition-guide/rust-2024/rustfmt-style-edition.html
- Rust lints : https://doc.rust-lang.org/rustc/lints/
- Clippy : https://doc.rust-lang.org/clippy/
- Rustonomicon : https://doc.rust-lang.org/nomicon/
- rustup toolchains : https://rust-lang.github.io/rustup/concepts/toolchains.html
- rustup components : https://rust-lang.github.io/rustup/concepts/components.html

### 40.2 Outils qualité

- cargo-deny : https://embarkstudios.github.io/cargo-deny/
- RustSec : https://rustsec.org/
- cargo-audit : https://github.com/rustsec/rustsec/tree/main/cargo-audit
- cargo-machete : https://docs.rs/crate/cargo-machete/latest
- cargo-udeps : https://github.com/est31/cargo-udeps
- cargo-hack : https://docs.rs/crate/cargo-hack/latest
- cargo-semver-checks : https://docs.rs/crate/cargo-semver-checks/latest
- cargo-nextest : https://nexte.st/
- cargo-llvm-cov : https://docs.rs/crate/cargo-llvm-cov/latest
- cargo-fuzz : https://rust-fuzz.github.io/book/cargo-fuzz.html
- proptest : https://docs.rs/proptest/latest/proptest/
- Miri : https://github.com/rust-lang/miri
- Loom : https://docs.rs/loom/latest/loom/
- Kani : https://model-checking.github.io/kani/
- cargo-geiger : https://docs.rs/crate/cargo-geiger/latest

### 40.3 Observabilité

- Tokio tracing topic : https://tokio.rs/tokio/topics/tracing
- tracing crate : https://docs.rs/tracing/latest/tracing/

---

## 41. Synthèse finale

Cette V2 formalise une exigence simple : le code Rust d’Andromeda doit être traité comme un actif d’ingénierie critique.

La qualité ne doit pas dépendre de la mémoire des développeurs. Elle doit être portée par :

```text
architecture
types
frontières
tests
CI
lints
documentation
review
mesure
observabilité
suppression active
```

La trajectoire recommandée est donc :

```text
1. Stabiliser la baseline Rust 2026.
2. Verrouiller le workspace.
3. Réduire les surfaces publiques.
4. Supprimer le mort.
5. Découper par responsabilités.
6. Consolider les concepts stables.
7. Renforcer les tests.
8. Mesurer avant optimiser.
9. Encapsuler unsafe.
10. Industrialiser les quality gates.
```

Le résultat attendu n’est pas seulement un dépôt plus propre. C’est un dépôt qui peut grandir sans perdre sa lisibilité, sa sécurité, sa performance et sa capacité à être audité.



---

## 42. Migration contrôlée vers Rust 2024 / baseline 2026

### 42.1 Objectif

La migration d’un workspace existant vers Rust 2024 ne doit pas être traitée comme un simple remplacement de chaîne dans `Cargo.toml`. Elle doit être conduite comme une migration de langage, d’outillage et de style.

### 42.2 Ordre recommandé

```text
1. Stabiliser main.
2. Vérifier que tous les tests passent avant migration.
3. Mettre à jour rust-toolchain.toml.
4. Mettre à jour rustfmt.toml avec style_edition = "2024".
5. Mettre à jour edition = "2024" crate par crate si besoin.
6. Déclarer resolver = "3" au workspace.
7. Déclarer rust-version.
8. Lancer cargo fix --edition.
9. Lancer cargo fmt.
10. Lancer check/clippy/test.
11. Corriger les migrations manuelles.
12. Valider all-features/all-targets.
13. Documenter la migration.
```

### 42.3 Commandes indicatives

```bash
cargo fix --edition --workspace --all-targets
cargo fmt --all
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
```

### 42.4 Migration feature-aware

`cargo fix --edition` ne couvre pas forcément tous les chemins `cfg` et toutes les features. Pour un workspace avancé :

```bash
cargo hack check --workspace --each-feature
cargo hack check --workspace --feature-powerset --depth 2
```

À adapter selon coût.

### 42.5 Critères de réussite

```text
- tous les packages ciblés sont en edition 2024 ;
- resolver 3 actif au workspace ;
- style_edition 2024 versionné ;
- rust-version documenté ;
- CI verte ;
- aucun diff de formatage instable entre IDE et CI ;
- migration documentée dans docs/decisions.
```

---

## 43. Politique API publique et compatibilité

### 43.1 API publique Rust

Une API publique de crate doit être considérée comme un contrat.

Elle inclut :

- types `pub`;
- fonctions `pub`;
- traits `pub`;
- modules `pub`;
- features ;
- erreurs publiques ;
- formats de sérialisation ;
- comportements documentés ;
- invariants testés.

### 43.2 Stabilité par criticité

| Niveau | Stabilité attendue |
|---|---|
| Expérimental | breaking changes acceptés, préfixe clair. |
| Interne | stabilité limitée au workspace. |
| Stable interne | changement via DEC/ADR. |
| Public client | compatibilité stricte. |
| Format persistant | compatibilité critique, migration obligatoire. |
| Protocole réseau | compatibilité versionnée. |

### 43.3 Règles de breaking change

Un changement est breaking si :

- un type public change de nom ;
- une fonction publique change de signature ;
- une erreur publique disparaît ;
- une feature change de sémantique ;
- un format binaire change sans version ;
- un contrat RPC change sans politique ;
- une cardinalité change ;
- une valeur par défaut change ;
- une trace critique change de champ obligatoire.

### 43.4 cargo-semver-checks

Pour les crates à API stable :

```bash
cargo semver-checks
```

Le but n’est pas de transformer toutes les crates internes en bibliothèques publiques, mais de détecter les changements involontaires.

---

## 44. Politique de nommage

### 44.1 Noms de crates

Format recommandé :

```text
andromeda-<domain>
```

Exemples :

```text
andromeda-wal
andromeda-storage
andromeda-contract
andromeda-security
```

Éviter :

```text
andromeda-common
andromeda-utils
andromeda-base
```

### 44.2 Noms de modules

Un module doit être nommé par concept :

```text
manifest
page
frame
contract
descriptor
validation
canonicalization
```

Éviter :

```text
helpers
misc
stuff
manager
processor
handler
```

`manager`, `processor`, `handler` ne sont pas interdits, mais ils sont souvent trop vagues. Ils doivent être précisés :

```text
wal_segment_writer
contract_compatibility_checker
rpc_frame_decoder
```

### 44.3 Noms de fonctions

Une fonction doit exprimer un verbe et un objet.

Préférer :

```rust
validate_manifest_hash(...)
encode_frame_header(...)
apply_catalog_batch(...)
publish_snapshot_manifest(...)
```

À éviter :

```rust
process(...)
handle(...)
do_work(...)
run(...)
check(...)
```

### 44.4 Noms booléens

Préférer :

```rust
is_valid
has_value
can_commit
should_retry
must_flush
```

Éviter les doubles négations :

```rust
is_not_invalid
disable_not_allowed
```

### 44.5 Noms d’erreurs

Format recommandé :

```rust
InvalidManifestHash
TruncatedWalRecord
ContractHashMismatch
PermissionDenied
PayloadTooLarge
```

Un nom d’erreur doit être exploitable dans logs, traces et diagnostics.

---

## 45. Architecture des tests pour Andromeda

### 45.1 `andromeda-testing`

Créer une crate dédiée aux helpers de tests transverses.

Elle peut contenir :

```text
fixtures
builders
temporary directories
fake clocks
deterministic ids
crash harness helpers
protocol frame generators
wal record generators
assertions métier
```

Elle ne doit pas contenir de logique de production.

### 45.2 Builders typés

Préférer :

```rust
let manifest = DatabaseManifestBuilder::new()
    .with_database_id(DatabaseId::new(1))
    .with_snapshot_id(SnapshotId::new(42))
    .build_valid();
```

À éviter :

```rust
let manifest = make_manifest(true, false, 1, 42, None);
```

### 45.3 Fixtures

Les fixtures doivent être :

- petites ;
- nommées ;
- versionnées ;
- documentées ;
- validées ;
- supprimées si obsolètes.

### 45.4 Golden files

Utiles pour :

- diagnostics SRPL ;
- formats textuels ;
- manifests ;
- frames ;
- erreurs sérialisées.

Règle :

```text
Un golden file doit être révisé comme une API.
```

### 45.5 Snapshots

Les snapshots de tests doivent être stockés dans :

```text
tests/snapshots/
```

Avec convention de nom :

```text
<crate>__<feature>__<scenario>.snap
```

---

## 46. Architecture CI recommandée

### 46.1 Pipeline minimal

```text
format
check
clippy
test
doc
audit
deny
```

### 46.2 Pipeline détaillé

```yaml
quality:
  - cargo fmt --all --check
  - cargo check --workspace --all-targets --all-features
  - cargo clippy --workspace --all-targets --all-features -- -D warnings
  - cargo test --workspace --all-targets --all-features
  - cargo doc --workspace --all-features --no-deps

security:
  - cargo deny check
  - cargo audit

dependencies:
  - cargo tree -d
  - cargo machete
  - cargo +nightly udeps --workspace --all-targets

features:
  - cargo hack check --workspace --each-feature

coverage:
  - cargo llvm-cov nextest --workspace --all-features

advanced:
  - cargo miri test -p critical-crate
  - cargo fuzz run target -- -max_total_time=60
```

### 46.3 Matrice OS/architecture

Pour Andromeda :

| Dimension | Recommandation |
|---|---|
| Linux x64 | obligatoire. |
| Linux ARM64 | obligatoire à terme. |
| Windows x64 | selon cible développeur/outillage. |
| macOS | utile pour contributeurs, pas forcément prod. |
| GPU | pipeline séparé, non bloquant V0. |
| nightly | pipeline séparé. |

### 46.4 CI rapide vs CI complète

CI rapide :

```text
fmt + check + clippy + tests principaux
```

CI complète :

```text
all-features + nextest + audit + deny + coverage + fuzz smoke + miri ciblé
```

---

## 47. Politique de branches et releases

### 47.1 Branche principale

`main` doit rester :

```text
compilable
testée
sans warnings
sans dette silencieuse
```

### 47.2 Branches de refactorisation

Nom recommandé :

```text
refactor/<crate>-<subject>
cleanup/<scope>
quality/<tooling>
```

### 47.3 Changelog interne

Chaque release interne doit indiquer :

```text
Added
Changed
Fixed
Removed
Security
Performance
Breaking
Migration
```

### 47.4 Release de crate interne

Avant de déclarer une crate stable :

- README crate ;
- rustdoc ;
- tests ;
- API revue ;
- lints ;
- semver check ;
- dépendances minimales ;
- owner identifié.

---

## 48. Gouvernance des crates critiques

### 48.1 Définition C5

Une crate est C5 si une erreur peut produire :

- perte de données ;
- corruption durable ;
- faille sécurité ;
- violation de transaction ;
- recovery impossible ;
- exposition de secret ;
- split-brain ;
- commit visible non durable.

### 48.2 Crates C5 candidates

```text
andromeda-wal
andromeda-storage
andromeda-tx
andromeda-security
andromeda-rpc
andromeda-catalog
andromeda-contract
```

### 48.3 Exigences C5

```text
no panic runtime
unsafe audité
formats versionnés
fuzz/property tests
crash tests si durable
traces structurées
erreurs typées
review spécialisée
rollback path
```

### 48.4 Interdiction de complexité non justifiée

Dans C5, la simplicité prouvable est supérieure à l’élégance abstraite.

---

## 49. Patterns Rust recommandés

### 49.1 Newtype

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lsn(u64);

impl Lsn {
    pub const ZERO: Self = Self(0);

    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}
```

### 49.2 Builder pour objets complexes

```rust
pub struct FrameHeaderBuilder {
    frame_type: FrameType,
    request_id: RequestId,
    payload_length: PayloadLength,
}

impl FrameHeaderBuilder {
    pub fn new(frame_type: FrameType) -> Self {
        Self {
            frame_type,
            request_id: RequestId::new(),
            payload_length: PayloadLength::ZERO,
        }
    }

    pub fn with_payload_length(mut self, length: PayloadLength) -> Self {
        self.payload_length = length;
        self
    }

    pub fn build(self) -> FrameHeader {
        FrameHeader {
            frame_type: self.frame_type,
            request_id: self.request_id,
            payload_length: self.payload_length,
        }
    }
}
```

### 49.3 Typestate

Utile pour empêcher certaines transitions invalides.

```rust
pub struct Transaction<S> {
    id: TxId,
    state: S,
}

pub struct Active;
pub struct Committing;
pub struct Committed;

impl Transaction<Active> {
    pub fn commit(self) -> Transaction<Committing> {
        Transaction {
            id: self.id,
            state: Committing,
        }
    }
}
```

À utiliser avec prudence : le typestate peut devenir lourd si trop généralisé.

### 49.4 Trait minimal

Un trait doit représenter un comportement stable.

```rust
pub trait Clock {
    fn now(&self) -> EngineTimestamp;
}
```

À éviter :

```rust
pub trait Manager {
    fn process(&self);
}
```

### 49.5 Sealed trait

Pour empêcher les implémentations externes non contrôlées :

```rust
mod sealed {
    pub trait Sealed {}
}

pub trait EngineType: sealed::Sealed {
    fn type_name(&self) -> &'static str;
}
```

---

## 50. Patterns à limiter

### 50.1 Trop de traits

Rust permet des abstractions puissantes, mais chaque trait ajoute un contrat.

Éviter de créer un trait si :

- une seule implémentation existe ;
- il ne représente pas un concept stable ;
- il sert seulement à faciliter un test ;
- il cache une dépendance de domaine ;
- il complique les erreurs.

### 50.2 Trop de génériques

Éviter :

```rust
fn process<T, U, V, F, E>(...)
```

si le gain n’est pas prouvé.

### 50.3 Macros prématurées

Les macros doivent être justifiées par :

- réduction d’erreurs répétitives ;
- génération de code canonique ;
- DSL contrôlé ;
- réduction de boilerplate dangereux.

Ne pas utiliser de macro pour masquer une architecture confuse.

### 50.4 Global state

À éviter :

```rust
static GLOBAL_ENGINE: OnceCell<Engine> = ...
```

Sauf décision formelle.

---

## 51. Runbooks qualité

### 51.1 Runbook — suppression d’une dépendance

```text
1. Identifier pourquoi elle existe.
2. cargo tree -i dependency.
3. Chercher usages directs.
4. Désactiver feature éventuelle.
5. Supprimer du Cargo.toml.
6. cargo check all-targets all-features.
7. cargo test.
8. cargo deny/audit.
9. Documenter suppression si notable.
```

### 51.2 Runbook — split d’un fichier lourd

```text
1. Lire le fichier entier.
2. Lister les responsabilités.
3. Identifier types publics.
4. Identifier fonctions pures.
5. Identifier effets de bord.
6. Créer sous-modules nommés.
7. Déplacer tests proches.
8. Réduire imports.
9. Réduire visibilité.
10. Relancer gates.
```

### 51.3 Runbook — réduction de `pub`

```text
1. Activer unreachable_pub.
2. Lister exports publics.
3. Identifier consommateurs.
4. Passer à pub(crate) si possible.
5. Passer à privé si possible.
6. Corriger tests.
7. Documenter API restante.
```

### 51.4 Runbook — audit unsafe

```text
1. cargo geiger.
2. Lister unsafe par crate.
3. Vérifier commentaire SAFETY.
4. Vérifier tests.
5. Vérifier alternatives safe.
6. Vérifier invariants.
7. Ajouter Miri/fuzz si possible.
8. Review spécialisée.
```

---

## 52. Critères de rejet d’une contribution

Une contribution doit être rejetée ou renvoyée si :

- elle ajoute une dépendance sans justification ;
- elle ajoute `unsafe` sans preuve ;
- elle ajoute des `unwrap` runtime ;
- elle crée une crate vague ;
- elle crée une API publique inutile ;
- elle mélange refactorisation et feature ;
- elle ajoute des tests ignorés ;
- elle casse la documentation ;
- elle diminue l’observabilité ;
- elle introduit un format implicite ;
- elle ne passe pas les gates ;
- elle contourne une frontière architecturale.

---

## 53. Standard minimal pour un nouveau module

Un nouveau module doit fournir :

```text
nom clair
responsabilité
frontières
types principaux
erreurs
tests
documentation si public
pas de dépendance inutile
pas de visibilité excessive
```

Exemple de header documentaire :

```rust
//! WAL segment encoding and validation.
//!
//! This module owns the binary representation of WAL segment headers.
//! It does not decide transaction visibility.
//!
//! Invariants:
//! - all encoded headers use canonical little-endian;
//! - invalid lengths are rejected before allocation;
//! - checksum mismatch returns `WalError::ChecksumMismatch`.
```

---

## 54. Standard minimal pour une nouvelle crate

Une nouvelle crate doit fournir :

```text
README.md
Cargo.toml propre
lib.rs fin
responsabilité unique
owners
dépendances justifiées
lints workspace
tests minimaux
docs publiques
critères de sortie
```

Question obligatoire :

```text
Pourquoi ce code ne doit-il pas rester un module dans une crate existante ?
```

Si la réponse est seulement « pour ranger », refuser.

---

## 55. Standard pour benchmarks

### 55.1 Objectif

Un benchmark doit répondre à une question.

Mauvais :

```text
bench speed
```

Bon :

```text
Comparer encode_frame_header avant/après suppression d’une allocation.
```

### 55.2 Données

Les données doivent être :

- représentatives ;
- versionnées ;
- petites pour CI rapide ;
- plus larges pour perf suite ;
- documentées.

### 55.3 Résultat

Un benchmark doit produire :

```text
scenario
input size
environment
metric
baseline
new value
delta
decision
```

---

## 56. Standard pour fuzzing

### 56.1 Cibles prioritaires

```text
decode_frame_header
decode_wal_record
decode_page_header
parse_srpl
decode_structured_object
decode_manifest
canonicalize_contract
```

### 56.2 Règle

Le fuzzing doit rechercher :

```text
panic
OOM
infinite loop
invalid accept
valid reject
non deterministic result
```

### 56.3 Corpus

Le corpus initial doit contenir :

- cas valide minimal ;
- cas valide maximal ;
- longueur tronquée ;
- magic invalide ;
- version inconnue ;
- checksum invalide ;
- payload oversized ;
- champs incohérents.

---

## 57. Standard pour diagnostics

### 57.1 Diagnostic développeur

Un diagnostic doit dire :

```text
quoi
où
pourquoi
comment corriger
sévérité
code stable
```

### 57.2 Exemple SRPL

```text
SRPL-CARD-001: cardinality `one Customer` is not guaranteed.
Location: procedure Sales.GetCustomer, line 17, column 9.
Reason: predicate does not use a unique access path.
Fix: constrain by Customer.Id or declare `optional one` / `many`.
```

### 57.3 Diagnostic Rust interne

Même pour les outils internes, éviter les messages opaques :

```text
invalid frame
```

Préférer :

```text
RPC-FRAME-003: payload length 4294967295 exceeds configured session limit 1048576.
```

---

## 58. Gouvernance des documents

### 58.1 Documents vivants

Un document vivant doit être :

- maintenu ;
- relié à un owner ;
- mis à jour lors des changements ;
- non contradictoire avec le code.

### 58.2 Archive

Les documents obsolètes doivent aller dans :

```text
docs/archive/
```

avec :

```text
superseded_by
date
reason
```

### 58.3 Interdiction

Ne pas laisser deux documents contradictoires actifs.

---

## 59. Grille de maturité du dépôt

| Niveau | Description |
|---|---|
| M0 | Code compile localement, structure instable. |
| M1 | Workspace Cargo propre, fmt/check/test. |
| M2 | Clippy, tests, docs minimales, dépendances connues. |
| M3 | Quality gates CI, architecture crates claire, clean-up régulier. |
| M4 | Fuzz/property tests, audit, coverage, performance mesurée. |
| M5 | Crash/recovery tests, unsafe audité, formats versionnés, gouvernance C5. |

Objectif Andromeda V0 :

```text
M3 général
M4 sur protocole/parser/storage
M5 sur WAL/recovery/transaction/security
```

---

## 60. Conclusion opérationnelle

Cette charte doit devenir un outil de travail quotidien.

Elle sert à :

- cadrer les PR ;
- refuser les dérives ;
- planifier les clean-ups ;
- standardiser les reviews ;
- structurer les crates ;
- mesurer la qualité ;
- éviter le code mort ;
- protéger les zones critiques ;
- rendre le projet durable.

Le standard attendu n’est pas « Rust idiomatique » au sens vague. Le standard attendu est :

```text
Rust professionnel,
architecture explicite,
qualité exécutable,
performance mesurée,
sécurité auditée,
refactorisation continue,
suppression active,
invariants vérifiables.
```
