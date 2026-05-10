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
