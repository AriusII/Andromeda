# Chemin critique

## Chemin principal

```text
P00 -> P01 -> P02 -> P03 -> P04 -> P05 -> P06 -> P07 -> P08 -> P09 -> P10 -> P11 -> P12 -> P13 -> P14 -> P15
```

## Chemin réellement bloquant

Le chemin critique dur n’est pas toute la roadmap. Il est :

```text
P00 état réel
-> P01 specs
-> P02 vertical durable
-> P03 catalog durable
-> P04 execution generic
-> P05 storage complete
-> P06 transaction/MVCC
-> P07 security/audit
-> P13 backup/restore/PITR
-> P15 release evidence
```

P08, P09, P10, P11 et P12 peuvent avancer en parallèle sous conditions, mais ne doivent jamais contourner le chemin dur.

## Décisions de parallélisation

| Peut avancer en parallèle | Condition stricte |
|---|---|
| P08 QUIC runtime | Admission, frame specs et Procedure dispatch stables. |
| P09 Stats/Optimizer | CatalogVersion, Procedure Store et DecisionTrace disponibles. |
| P10 Maps | Storage, catalog, stats et MapDescriptor stables. |
| P11 CPU/NVMe perf | Pas de changement sémantique durable. |
| P12 GPU | CPU fallback et no-C5 dependency scan. |

## Critère d’arrêt

On arrête ou on rétrograde une phase si elle exige une vérité qui n’existe pas encore. Exemple : un optimizer qui choisit un plan sans StatsVersion publiée doit être rejeté, pas compensé par un fallback invisible.
