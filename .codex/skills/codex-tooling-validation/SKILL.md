---
name: codex-tooling-validation
description: "Use to validate .codex agents, skills, registries and routing files."
category: codex-tooling
---

# codex-tooling-validation

## When to use
Before completing changes to .codex tooling.

## Purpose
Use to validate .codex agents, skills, registries and routing files.

## Process
- Run `.codex/scripts/validate_codex_tooling.py` from repository root.
- Check all agent files are registered.
- Check all skill names match folder names.
- Check referenced primary skills exist.
- Check no worker output rule is contradicted.

## Expected output
- Validation command and pass/fail summary.

## Guardrails
- Do not claim validation if the script was not run.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
