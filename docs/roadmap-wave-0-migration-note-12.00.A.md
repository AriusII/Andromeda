# Wave-0 — Lot 12.00.A (Checkpoint)

## 12.00.A — Migration note (concise)

### Actions réalisées pendant le lot

- Snapshot Wave-0 reconcilé : inventaire `MM` / `AD` / `A` / `??` arrêté et priorisé par ownership avant toute extraction métier.
- Topologie de base confirmée : `cargo metadata --no-deps --format-version 1` retourne `94` membres workspace.
- Périmètre `fuzz/` stabilisé en exécution : compile des cibles et génération de corpus validées depuis `fuzz/`.
- Décision de propriété (phase Wave-0) :  
  - `andromeda-rpc-protocol` maintient l’ownership protocol/runtime-free.  
  - `andromeda-protocol` reste scaffold tant que l’ownership preuve n’est pas objectivée.
- Définition de contrôle W30 : la sortie externe reste de type "évidence de snapshot + décisions de gouvernance", sans claim d’acceptation C5 complète.

### Prochains contrôles

- Fermer proprement les diffs MM/AD et produire le checkpoint Wave-0 avec zéro point actif.
- Exécuter le lot de gates Wave-0 sur la branche réconciliée : `fmt`, `clippy`, `nextest`, `test --doc`, `cargo audit`, `cargo deny`.
- Prouver C5 Wave-0 avec matrix `unit + integration + targeted fuzz + crash/recovery + doctrine scan` et conserver les rapports horodatés.
- Lancer le contrôle de migration suivant (12.00.B) uniquement après validation complète des contrôles ci-dessus.
