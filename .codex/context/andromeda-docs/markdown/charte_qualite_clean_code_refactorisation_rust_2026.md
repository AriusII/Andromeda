# Charte générique — Qualité, Clean Code, Refactorisation et Architecture Rust

> **Contexte**  
> Ce document formalise une volonté de développement Rust professionnel : code propre, architecture claire, refactorisation profonde, suppression du code mort, consolidation des responsabilités, découpage fin des fichiers, gestion stricte des crates, tests, performances, dépendances et documentation.
>
> Il est volontairement générique afin de pouvoir être placé dans un dépôt Rust comme document de cadrage, charte qualité, guide de contribution ou base de revue d’architecture.

---

## 1. Objectif général

Le projet doit appliquer une politique stricte de qualité de code Rust visant à garantir :

- un code lisible, maintenable, auditable et durable ;
- une séparation claire des responsabilités ;
- une architecture modulaire par workspace, crates, modules, dossiers et sous-dossiers ;
- une réduction active de la dette technique ;
- une suppression régulière du code mort, des fichiers inutilisés, des dépendances inutiles et des éléments orphelins ;
- une refactorisation continue, progressive et contrôlée ;
- une performance mesurée, maîtrisée et justifiée ;
- une surface publique minimale, documentée et stable ;
- une qualité homogène entre `src/`, `tests/`, `benches/`, `examples/`, `docs/` et les outils internes.

L’objectif n’est pas seulement de produire un code qui fonctionne, mais de construire une base Rust saine, professionnelle, performante, correctement découpée, testable, vérifiable automatiquement et exploitable dans un contexte long terme.

---

## 2. Principes directeurs

### 2.1 Clean Code

Le code doit être :

- **clair** : chaque module, type, fonction ou trait doit avoir une responsabilité identifiable ;
- **minimal** : aucun élément inutile, redondant ou spéculatif ne doit être conservé ;
- **localement compréhensible** : un fichier doit pouvoir être compris sans nécessiter de parcourir toute la base de code ;
- **fortement typé** : les invariants métier ou techniques doivent être exprimés par les types autant que possible ;
- **explicite** : éviter les raccourcis ambigus, les noms vagues, les états implicites et les comportements cachés ;
- **testable** : le découpage doit permettre des tests unitaires, d’intégration et de non-régression efficaces ;
- **documenté là où c’est utile** : la documentation doit expliquer les invariants, les contrats, les limites, les décisions et les compromis, pas répéter mécaniquement le code.

### 2.2 Refactorisation

La refactorisation désigne toute modification qui améliore la structure interne du code sans changer son comportement observable.

Elle peut inclure :

- renommage de types, fonctions, modules, traits ou crates ;
- extraction de fonctions ;
- extraction de modules ;
- extraction de crates ;
- réduction de duplication ;
- simplification de branches conditionnelles ;
- remplacement d’un type trop générique par un type métier fort ;
- réduction de la visibilité publique ;
- clarification des erreurs ;
- séparation entre logique pure, I/O, persistance, réseau, parsing, configuration et orchestration ;
- amélioration de la testabilité ;
- suppression d’abstractions prématurées ;
- consolidation d’abstractions répétées ;
- réduction du couplage entre crates ;
- amélioration de la cohérence de nommage ;
- regroupement des concepts dispersés.

### 2.3 Suppression du code mort

Le code mort doit être supprimé, pas simplement ignoré.

Cela concerne :

- fonctions non appelées ;
- types non instanciés ;
- traits inutilisés ;
- modules non référencés ;
- fichiers `.rs` non reliés à l’arbre de modules ;
- feature flags inutilisées ;
- tests obsolètes, ignorés ou désactivés sans justification ;
- dépendances présentes dans `Cargo.toml` mais non utilisées ;
- exemples non maintenus ;
- documentation qui ne correspond plus au comportement réel ;
- anciens chemins de migration devenus inutiles ;
- wrappers, adapters, helpers ou utilities créés pour des besoins disparus ;
- scripts internes non utilisés ;
- fichiers de configuration historiques sans rôle actuel.

---

## 3. Organisation Rust attendue

Cargo impose déjà des conventions reconnues pour l’organisation des packages Rust : `src/lib.rs`, `src/main.rs`, `src/bin/`, `tests/`, `benches/`, `examples/`, etc. Ces conventions doivent être respectées avant d’ajouter une organisation personnalisée.

### 3.1 Structure workspace recommandée

```text
repository/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── rustfmt.toml
├── clippy.toml
├── deny.toml
├── README.md
├── CONTRIBUTING.md
├── ARCHITECTURE.md
├── crates/
│   ├── core/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── tests/
│   ├── domain/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── tests/
│   ├── protocol/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── tests/
│   ├── storage/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── tests/
│   ├── runtime/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── tests/
│   └── cli/
│       ├── Cargo.toml
│       ├── src/
│       └── tests/
├── tests/
│   ├── integration/
│   ├── fixtures/
│   └── snapshots/
├── benches/
├── examples/
├── docs/
│   ├── architecture/
│   ├── decisions/
│   ├── quality/
│   ├── testing/
│   └── performance/
├── scripts/
├── tools/
└── xtask/
```

### 3.2 Rôle des dossiers

| Dossier | Rôle |
|---|---|
| `crates/` | Contient les crates internes du workspace. |
| `src/` | Code source principal d’une crate. |
| `tests/` | Tests d’intégration propres à une crate ou au workspace. |
| `benches/` | Benchmarks reproductibles. |
| `examples/` | Exemples compilables, maintenus et représentatifs. |
| `docs/` | Documentation d’architecture, décisions, conventions et procédures. |
| `scripts/` | Scripts utilitaires non critiques. |
| `tools/` | Outils internes, générateurs ou validateurs. |
| `xtask/` | Automatisation Rust-native des tâches de build, audit, génération ou nettoyage. |

---

## 4. Découpage des crates

Un workspace Cargo permet de gérer plusieurs packages liés développés ensemble. Cette structure est adaptée lorsqu’un projet grandit et nécessite plusieurs crates maintenues dans un même ensemble cohérent.

### 4.1 Une crate doit avoir une responsabilité claire

Une crate ne doit pas devenir un dossier fourre-tout.

Exemples de responsabilités acceptables :

- `domain` : types métier, invariants, règles pures ;
- `storage` : abstraction de stockage, pages, fichiers, buffers ;
- `protocol` : framing, sérialisation, désérialisation, contrats réseau ;
- `runtime` : orchestration, exécution, scheduling ;
- `cli` : interface ligne de commande ;
- `testing-support` : utilitaires de tests partagés ;
- `benchmarking` : workloads, mesures, scénarios de performance ;
- `observability` : métriques, tracing, diagnostics, instrumentation ;
- `config` : chargement, validation et normalisation de configuration ;
- `errors` : erreurs transverses, si une stratégie commune est réellement nécessaire.

### 4.2 Critères d’extraction d’une nouvelle crate

Créer une nouvelle crate est pertinent si :

- le module possède une responsabilité autonome ;
- le code peut être testé indépendamment ;
- les dépendances sont spécifiques à ce périmètre ;
- la compilation peut bénéficier d’une séparation nette ;
- l’API interne devient suffisamment stable ;
- le module est réutilisé par plusieurs autres crates ;
- le découpage réduit la complexité cognitive globale ;
- la frontière de dépendances devient plus saine ;
- la crate peut porter une documentation claire et autonome.

Ne pas créer une crate uniquement pour “ranger” quelques fichiers. Une crate introduit une frontière d’API, de dépendances, de compilation et de responsabilité.

### 4.3 Dépendances entre crates

Les dépendances doivent former un graphe clair.

À éviter :

- dépendances circulaires conceptuelles ;
- crates techniques dépendant de crates applicatives ;
- crate `core` devenant une poubelle globale ;
- crate `utils` contenant des responsabilités sans lien ;
- dépendances ajoutées pour contourner un mauvais découpage ;
- exposition publique excessive pour résoudre un problème d’accès interne.

À privilégier :

- dépendances descendantes ;
- APIs petites ;
- types de domaine stables ;
- contrats explicites ;
- modules internes privés ;
- séparation nette entre logique pure et effets de bord.

---

## 5. Découpage des modules et fichiers

Le système de modules Rust permet de contrôler l’organisation, les chemins, les imports et la visibilité via `mod`, `use`, `pub`, `pub(crate)` et `pub(super)`. La visibilité doit être pensée comme un contrat, pas comme un simple mécanisme d’accès.

### 5.1 Règles générales

Un fichier Rust doit rester :

- court ;
- cohérent ;
- centré sur une responsabilité ;
- facile à parcourir ;
- sans mélange de couches ;
- sans accumulation de fonctions sans rapport direct ;
- testable ;
- correctement nommé ;
- relié explicitement à l’arbre de modules.

### 5.2 Seuils internes proposés

Ces seuils ne sont pas des règles officielles Rust, mais des garde-fous de qualité.

| Élément | Seuil recommandé | Action si dépassé |
|---|---:|---|
| Fichier `.rs` | 300 à 500 lignes | Extraire en sous-modules. |
| Fonction simple | 30 à 50 lignes | Extraire des étapes nommées. |
| Fonction complexe | 50 à 80 lignes | Refactoriser prioritairement. |
| Type struct/enum | Trop de champs ou variantes | Séparer les responsabilités. |
| Module | Trop de concepts mélangés | Créer des sous-modules. |
| Crate | Trop de domaines différents | Extraire une crate dédiée. |

Ces seuils ne doivent pas être appliqués mécaniquement, mais tout dépassement durable doit déclencher une revue.

### 5.3 Exemple de découpage interne

```text
src/
├── lib.rs
├── error.rs
├── config/
│   ├── mod.rs
│   ├── loader.rs
│   ├── validation.rs
│   └── defaults.rs
├── model/
│   ├── mod.rs
│   ├── identifiers.rs
│   ├── metadata.rs
│   └── state.rs
├── engine/
│   ├── mod.rs
│   ├── lifecycle.rs
│   ├── scheduler.rs
│   └── execution.rs
├── io/
│   ├── mod.rs
│   ├── reader.rs
│   ├── writer.rs
│   └── buffer.rs
└── tests_support/
    ├── mod.rs
    ├── builders.rs
    └── fixtures.rs
```

### 5.4 Visibilité

La visibilité doit être minimale par défaut.

Préférer :

```rust
pub(crate)
```

à :

```rust
pub
```

lorsqu’un élément n’a pas vocation à sortir de la crate.

Principes :

- `pub` expose un contrat stable.
- `pub(crate)` expose un détail interne à la crate.
- `pub(super)` limite l’usage au module parent.
- privé par défaut reste la meilleure option tant qu’aucun besoin réel n’existe.

Une API publique doit être considérée comme coûteuse : elle impose de la stabilité, de la documentation, des tests et de la prudence.

---

## 6. Détection des orphelins

### 6.1 Types d’orphelins

Un élément orphelin est un élément présent dans le dépôt mais qui n’a plus de rôle réel.

| Type | Définition |
|---|---|
| Fichier orphelin | Fichier non référencé par l’arbre de modules ou par Cargo. |
| Module orphelin | Module déclaré mais inutile, ou fichier présent mais jamais déclaré. |
| Dépendance orpheline | Crate listée dans `Cargo.toml` mais non utilisée. |
| Feature orpheline | Feature déclarée mais jamais activée ou testée. |
| Test orphelin | Test désactivé, obsolète ou ne validant plus un comportement utile. |
| Documentation orpheline | Documentation qui décrit une ancienne architecture. |
| Exemple orphelin | Exemple qui ne compile plus ou ne représente plus l’usage réel. |
| Benchmark orphelin | Benchmark non maintenu, non reproductible ou non interprétable. |
| Script orphelin | Script présent mais non documenté, non appelé et non maintenu. |
| Configuration orpheline | Fichier de configuration historique ou non utilisé. |

### 6.2 Politique

Chaque orphelin doit être :

1. supprimé ;
2. réintégré proprement ;
3. documenté comme temporaire avec une échéance ;
4. transformé en dette technique explicitement suivie.

Un orphelin silencieux ne doit pas rester dans le dépôt.

---

## 7. Nettoyage et consolidation

### 7.1 Nettoyage

Le nettoyage consiste à supprimer ou corriger :

- code mort ;
- imports inutiles ;
- dépendances inutiles ;
- fichiers inutilisés ;
- duplications ;
- TODO obsolètes ;
- warnings ;
- feature flags mortes ;
- configurations divergentes ;
- documentation périmée ;
- scripts non utilisés ;
- exemples cassés ;
- benchmarks obsolètes ;
- commentaires mensongers ;
- hacks temporaires devenus permanents.

### 7.2 Consolidation

La consolidation consiste à regrouper correctement des éléments dispersés qui expriment le même concept.

Elle peut viser :

- plusieurs fonctions similaires ;
- plusieurs types représentant la même idée ;
- plusieurs erreurs techniques mal normalisées ;
- plusieurs modules redondants ;
- plusieurs conventions de nommage concurrentes ;
- plusieurs chemins d’exécution identiques ;
- plusieurs helpers de test duplicatifs ;
- plusieurs conversions répétées ;
- plusieurs stratégies de validation concurrentes.

### 7.3 Factorisation prudente

La factorisation ne doit pas créer d’abstraction artificielle.

Une abstraction est justifiée si :

- elle réduit une duplication réelle ;
- elle nomme un concept stable ;
- elle améliore la lisibilité ;
- elle clarifie une frontière ;
- elle réduit les risques d’erreur ;
- elle ne cache pas un coût important ;
- elle ne rend pas le code plus générique que le besoin réel.

Une mauvaise abstraction peut être pire qu’une duplication locale temporaire.

---

## 8. Qualité Rust spécifique

### 8.1 Typage fort

Utiliser le système de types pour exprimer les invariants.

Préférer :

```rust
pub struct PageId(u64);
pub struct SegmentId(u32);
pub struct NonEmptyName(String);
```

à :

```rust
pub type PageId = u64;
pub type SegmentId = u32;
pub type Name = String;
```

Les alias de type sont acceptables pour améliorer la lisibilité, mais ils ne créent pas de distinction forte au niveau du typage. Les newtypes sont préférables lorsqu’il existe un invariant réel ou un risque de confusion entre deux valeurs de même représentation.

### 8.2 Erreurs

Les erreurs doivent être :

- typées ;
- explicites ;
- contextualisées ;
- non ambiguës ;
- testables ;
- exploitables par l’appelant.

Éviter les erreurs génériques qui masquent l’origine du problème.

Chaque erreur importante doit permettre de comprendre :

- où le problème s’est produit ;
- quel invariant a été violé ;
- si l’erreur est récupérable ;
- si l’erreur relève d’une entrée invalide, d’un bug, d’un état système ou d’un problème externe.

### 8.3 Ownership et emprunts

Les règles d’ownership doivent être utilisées pour :

- éviter les copies inutiles ;
- réduire les allocations ;
- clarifier la durée de vie des données ;
- limiter les états partagés ;
- rendre la concurrence plus sûre ;
- éviter les mutations globales implicites.

### 8.4 Allocation

Toute allocation significative doit être consciente.

Sur les chemins critiques :

- éviter les `String` temporaires inutiles ;
- préférer les slices lorsque possible ;
- préallouer avec capacité lorsque le volume est connu ;
- limiter les clones ;
- distinguer clairement copie volontaire et copie accidentelle ;
- mesurer avant d’optimiser lourdement.

### 8.5 API publique

Une API publique doit être :

- stable ;
- documentée ;
- minimale ;
- cohérente ;
- nommée selon les conventions Rust ;
- difficile à mal utiliser ;
- explicite sur ses erreurs ;
- explicite sur ses coûts ;
- compatible avec les futures évolutions raisonnables du projet.

---

## 9. Outillage qualité obligatoire

### 9.1 Formatage

Le formatage doit être automatisé avec `rustfmt`.

Commande recommandée :

```bash
cargo fmt --all --check
```

### 9.2 Linting

Clippy doit être utilisé pour détecter les erreurs fréquentes, les anti-patterns, les problèmes d’idiomaticité et les améliorations possibles.

Commande recommandée :

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

### 9.3 Compilation complète

```bash
cargo check --workspace --all-targets --all-features
```

### 9.4 Tests

```bash
cargo test --workspace --all-targets --all-features
```

### 9.5 Documentation

```bash
cargo doc --workspace --all-features --no-deps
```

### 9.6 Audit de dépendances

Commandes possibles :

```bash
cargo audit
cargo deny check
```

### 9.7 Nettoyage périodique

Commandes ou contrôles à envisager selon les outils retenus :

```bash
cargo tree
cargo tree --duplicates
cargo metadata
cargo update
```

Objectifs :

- détecter les dépendances transitives lourdes ;
- repérer les duplications de versions ;
- identifier les dépendances indirectes inattendues ;
- surveiller l’évolution du graphe de dépendances.

---

## 10. Politique de tests

### 10.1 Tests unitaires

Les tests unitaires doivent rester proches du module testé.

Objectifs :

- valider les invariants ;
- couvrir les branches critiques ;
- tester les erreurs ;
- tester les limites ;
- tester les cas minimaux et maximaux ;
- éviter de tester uniquement le cas heureux.

### 10.2 Tests d’intégration

Les tests d’intégration doivent vérifier les comportements publics entre crates, modules ou composants.

Emplacements possibles :

```text
crate-name/tests/
repository/tests/integration/
```

### 10.3 Fixtures

Les fixtures doivent être :

- nommées clairement ;
- versionnées ;
- minimales ;
- compréhensibles ;
- non redondantes ;
- isolées des tests qui les utilisent.

### 10.4 Benchmarks

Les benchmarks doivent être séparés des tests fonctionnels.

Objectifs :

- mesurer les chemins critiques ;
- détecter les régressions ;
- comparer des stratégies ;
- documenter les hypothèses de performance ;
- éviter les résultats non reproductibles ;
- produire des données interprétables ;
- éviter les benchmarks décoratifs sans décision associée.

---

## 11. Gestion du poids des fichiers

### 11.1 Fichiers trop lourds

Un fichier devient problématique lorsqu’il :

- mélange plusieurs responsabilités ;
- oblige à scroller excessivement ;
- contient plusieurs concepts indépendants ;
- cache des dépendances implicites ;
- contient trop de types publics ;
- contient trop de fonctions privées sans structure ;
- rend les tests difficiles à localiser ;
- rend les reviews longues et peu efficaces ;
- devient un point de conflit Git fréquent ;
- empêche une compréhension locale rapide.

### 11.2 Stratégie de split

Lorsqu’un fichier grossit, extraire par :

1. responsabilité ;
2. concept métier ;
3. couche technique ;
4. niveau d’abstraction ;
5. type principal ;
6. protocole ;
7. erreur ;
8. validation ;
9. conversion ;
10. tests support ;
11. instrumentation ;
12. construction ou builder ;
13. parsing ;
14. sérialisation ;
15. stratégie d’exécution.

### 11.3 Exemple

Avant :

```text
src/engine.rs
```

Après :

```text
src/engine/
├── mod.rs
├── lifecycle.rs
├── execution.rs
├── scheduler.rs
├── state.rs
├── error.rs
└── metrics.rs
```

---

## 12. Code review

Chaque revue de code doit contrôler :

- le comportement ;
- la lisibilité ;
- la structure ;
- la responsabilité des modules ;
- la visibilité publique ;
- les erreurs ;
- les allocations ;
- les clones ;
- les dépendances ;
- les tests ;
- la documentation ;
- les risques de régression ;
- les fichiers ou modules devenus orphelins ;
- la cohérence avec l’architecture existante ;
- la stabilité des APIs publiques ;
- la présence de dette technique non déclarée.

### 12.1 Questions de revue

- Ce code appartient-il à cette crate ?
- Ce fichier est-il encore lisible ?
- Cette fonction fait-elle trop de choses ?
- Ce type exprime-t-il correctement l’invariant ?
- Cette dépendance est-elle justifiée ?
- Cette abstraction est-elle nécessaire maintenant ?
- Ce code peut-il être supprimé ?
- Ce module devrait-il être privé ?
- Le comportement est-il testé ?
- Le coût mémoire ou CPU est-il explicite ?
- Le nommage est-il idiomatique Rust ?
- La documentation décrit-elle une décision utile ?
- Le changement augmente-t-il inutilement le couplage ?
- Le changement rend-il une future refactorisation plus difficile ?
- Les tests couvrent-ils les erreurs, pas seulement le succès ?

---

## 13. Politique de dépendances

Une dépendance externe doit être justifiée.

Critères :

- utilité réelle ;
- maturité ;
- maintenance ;
- sécurité ;
- licence ;
- impact sur la compilation ;
- impact sur la taille binaire ;
- dépendances transitives ;
- complexité introduite ;
- possibilité de remplacement interne simple ;
- stabilité de l’API ;
- compatibilité avec les plateformes cibles ;
- capacité à être auditée.

Une dépendance ne doit pas être ajoutée pour éviter d’écrire quelques lignes simples, sauf si elle apporte une garantie claire de sécurité, robustesse, conformité ou maintenabilité.

---

## 14. Politique de performance

### 14.1 Principes

La performance doit être :

- mesurée ;
- localisée ;
- justifiée ;
- documentée ;
- non destructrice pour la lisibilité ;
- validée par tests ou benchmarks.

### 14.2 À éviter

- micro-optimisation sans mesure ;
- usage excessif de `unsafe`;
- clones silencieux ;
- allocations cachées ;
- abstractions dynamiques inutiles ;
- conversions répétées ;
- parsing répété ;
- logs coûteux dans les chemins critiques ;
- contention non mesurée ;
- structures de données choisies par habitude ;
- optimisation qui rend le comportement plus fragile.

### 14.3 À privilégier

- types adaptés ;
- slices ;
- itérateurs simples ;
- préallocation ;
- séparation des chemins critiques ;
- réduction des copies ;
- structures compactes ;
- erreurs non allouantes si nécessaire ;
- benchmarks reproductibles ;
- instrumentation claire ;
- profilage avant optimisation ;
- documentation des compromis.

---

## 15. Politique sur `unsafe`

Le code `unsafe` doit rester exceptionnel.

Il doit être :

- isolé ;
- documenté ;
- justifié ;
- testé ;
- encapsulé derrière une API sûre ;
- relu avec attention ;
- accompagné d’un commentaire expliquant les invariants de sûreté.

Aucun bloc `unsafe` ne doit être introduit pour contourner temporairement le borrow checker sans justification technique solide.

Chaque usage de `unsafe` doit répondre à ces questions :

- pourquoi `unsafe` est-il nécessaire ?
- quel invariant garantit la sûreté ?
- comment cet invariant est-il testé ?
- l’API publique reste-t-elle sûre ?
- existe-t-il une alternative sûre suffisamment performante ?
- le périmètre `unsafe` est-il minimal ?

---

## 16. Dette technique

La dette technique doit être visible.

Chaque dette doit préciser :

- le problème ;
- la raison de son existence ;
- l’impact ;
- le risque ;
- le périmètre ;
- la stratégie de résolution ;
- la priorité ;
- l’échéance ou le déclencheur de traitement.

Un simple `TODO` sans contexte n’est pas suffisant.

Préférer :

```rust
// TECH-DEBT:
// Contexte : ce parser garde une branche temporaire pour l’ancien format.
// Risque : duplication de validation.
// Résolution : supprimer après migration complète des fixtures v1.
// Suivi : issue #123.
```

---

## 17. Documentation technique

La documentation doit couvrir :

- architecture générale ;
- découpage des crates ;
- responsabilités des modules ;
- décisions structurantes ;
- conventions de nommage ;
- politique de tests ;
- politique de performance ;
- politique de dépendances ;
- règles de review ;
- commandes de validation ;
- règles de contribution ;
- dette technique connue ;
- conventions de feature flags ;
- règles sur `unsafe`;
- stratégie de migration.

Documents recommandés :

```text
README.md
CONTRIBUTING.md
ARCHITECTURE.md
docs/decisions/ADR-0001-*.md
docs/quality/code-quality.md
docs/performance/performance-policy.md
docs/testing/testing-policy.md
docs/dependencies/dependency-policy.md
```

---

## 18. Definition of Done

Une modification est considérée terminée si :

- le code compile ;
- `cargo fmt` passe ;
- `cargo clippy` passe sans warning ;
- les tests passent ;
- les erreurs sont correctement typées ;
- les fichiers sont correctement rangés ;
- aucun code mort évident n’a été ajouté ;
- aucune dépendance inutile n’a été introduite ;
- la documentation utile est mise à jour ;
- les tests couvrent le comportement ajouté ou modifié ;
- les changements de structure sont cohérents avec l’architecture ;
- les performances critiques ne régressent pas sans justification ;
- les APIs publiques restent minimales et documentées ;
- les nouveaux modules ont une responsabilité claire ;
- les nouveaux fichiers ne créent pas d’orphelins ;
- la dette technique éventuelle est explicitement documentée.

---

## 19. Commande qualité complète proposée

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --all-features --no-deps
cargo audit
cargo deny check
```

---

## 20. Formulation synthétique réutilisable

Le projet applique une politique stricte de qualité Rust fondée sur le Clean Code, la refactorisation continue, la suppression active du code mort, la détection des éléments orphelins, la consolidation des responsabilités, la maîtrise des dépendances et l’optimisation mesurée.

L’architecture doit être organisée en workspace Cargo multi-crates, avec une séparation claire entre crates, modules, fichiers sources, tests, benchmarks, exemples, documentation et outils internes. Chaque crate doit porter une responsabilité identifiable. Chaque fichier doit rester lisible, ciblé et suffisamment court pour éviter l’accumulation de logique hétérogène.

Le code doit être régulièrement nettoyé, découpé, renommé, consolidé et simplifié afin de conserver une base saine, professionnelle, performante et maintenable. Les fichiers trop volumineux doivent être séparés en sous-modules cohérents. Les dépendances inutilisées, fichiers morts, modules non référencés, tests obsolètes, features inutilisées et documentations périmées doivent être supprimés ou explicitement traités comme dette technique.

La qualité doit être vérifiée automatiquement par `cargo fmt`, `cargo clippy`, `cargo check`, `cargo test`, la génération documentaire, les audits de dépendances et les revues de code. La performance doit être mesurée avant optimisation, et toute optimisation doit rester compatible avec la lisibilité, la sûreté et la maintenabilité du projet.

L’objectif final est de produire un code Rust propre, robuste, fortement typé, bien découpé, professionnel, auditable, performant et durable.

---

## 21. Références officielles utiles

- Cargo Workspaces : https://doc.rust-lang.org/cargo/reference/workspaces.html
- Cargo Package Layout : https://doc.rust-lang.org/cargo/guide/project-layout.html
- Rust modules, scope et privacy : https://doc.rust-lang.org/book/ch07-02-defining-modules-to-control-scope-and-privacy.html
- rustfmt : https://rust-lang.github.io/rustfmt/
- Clippy : https://doc.rust-lang.org/stable/clippy/
- Rust API Guidelines : https://rust-lang.github.io/api-guidelines/
- Rust 2024 Edition Guide : https://doc.rust-lang.org/edition-guide/rust-2024/
- Rust 1.85.0 et Rust 2024 : https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/
