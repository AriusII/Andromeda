# Méthode de réalisation et rubber duck technique

## Principe

Andromeda doit progresser par preuves, pas par accumulation de fonctionnalités. Chaque phase doit répondre à quatre questions avant d’être acceptée :

1. Qu’est-ce qui devient plus vrai, plus durable ou plus strict ?
2. Quelle preuve montre que cela fonctionne ?
3. Quel test montre que cela échoue proprement ?
4. Quelle trace permet de l’expliquer après coup ?

## Méthode PR

| Étape | Règle |
|---|---|
| Design note | Décrire owner crate, invariant, rejection criteria. |
| Test first | Ajouter au moins un test de succès et un test de rejet. |
| Implémentation | Changement minimal, sans nouvelle frontière implicite. |
| Evidence | Commandes et sorties conservées dans un rapport. |
| Documentation | Mettre à jour spec/phase/status si le comportement change. |

## P00 baseline execution rule

P00 work starts from repository truth, not from older documentation memory. The
root `Cargo.toml` is the authority for Rust 1.95.0, Rust 2024 Edition,
resolver 3, and 89 workspace crates. Any P00 claim that conflicts with
`Cargo.toml`, `docs/status.md`, or `docs/roadmap/00_CURRENT_STATE_CROSS_CHECK.md`
must be corrected or marked historical.

P00 may close repository-state governance while release readiness remains
closed. That distinction is mandatory: validation scripts, runbooks, and
release-evidence generators can establish shape and local readiness, but only
retained gate output can support a release claim.

## Gate status vocabulary

| Statut | Usage |
|---|---|
| `PASS` | La commande ou preuve exacte passe sur le commit, l'outil, la plateforme et le scope documente. |
| `FAIL` | La commande ou preuve exacte echoue et bloque le claim. |
| `BLOCKED` | La preuve, l'outil, le workflow ou l'artefact requis manque. |
| `SKIPPED` | Le gate n'a pas ete lance et possede une decision explicite avec risque residuel. |
| `NOT IN SCOPE` | Le gate ne s'applique pas au changement borne et la raison est documentee. |

## Release-operation evidence

Pour backup, restore, PITR, forensic startup, HA/DR, quorum, fencing et release
gates, Personne 20 doit relier chaque claim a trois elements :

1. un runbook operateur ou une procedure de validation ;
2. une commande ou un drill exact avec sortie conservee ;
3. une decision de release indiquant les gaps, exclusions et risques residuels.

Les scripts `backup_restore_drill_check.py`, `hadr_cluster_drill_check.py`,
`validation_manifest.py`, `roadmap_gate_summary.py` et `release_evidence.py`
sont des aides de preuve. Ils ne remplacent pas un drill de restauration, un
drill de promotion HA/DR, un transcript CI, un `RecoveryReport`, un
`RestoreTrace` ou une preuve d'audit retenue.

## Rubber duck des mauvaises décisions fréquentes

| Tentation | Pourquoi c’est dangereux | Réponse correcte |
|---|---|---|
| Ajouter un handler spécial pour aller plus vite. | Crée un second runtime non catalogué. | Ajouter ProcedureRegistry ou adapter explicite. |
| Utiliser JSON pour payload runtime. | Perte de typage/protocole et allocations difficiles à borner. | Utiliser StructuredObject/RPC frame typé. |
| Laisser le GPU produire stats directement publiées. | GPU devient indirectement vérité optimizer. | CPU validation + StatsVersion candidate. |
| Faire confiance à RAM ou BufferPool. | Recovery mensonger après crash. | WAL durable + manifest/snapshot. |
| Dire “ACID” sans anomalies. | Garantie imprécise. | Décrire isolation policy et anomalies interdites. |
| Gérer admin depuis Application Surface. | Bypass sécurité. | Surfaces QUIC séparées + SurfaceScope. |

## Exit evidence standard

```text
spec/ADR mis à jour
+ tests ciblés
+ negative tests
+ trace/evidence DTO
+ validation command
+ known gaps
```

## Phrase d’arbitrage

Quand deux options sont possibles, choisir celle qui rend la vérité plus reconstruisible, les contrats plus explicites, les erreurs plus typées et les décisions plus observables.
