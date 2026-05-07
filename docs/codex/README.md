# Andromeda Codex Tooling

**Package version:** 2026.05.07-rust-codex-tooling-v2  
**Date:** 2026-05-07

## Purpose

This folder documents the Codex tooling package for Andromeda.

The package is designed to be extracted at the root of the Andromeda repository. It provides:

- Root and scoped `AGENTS.md` files.
- A Codex agent registry.
- Specialized Codex agents.
- Specialized Codex skills.
- Hooks and hook scripts.
- Prompt templates.
- Validation scripts.
- Routing metadata.

## Design principle

Agents and skills are intentionally different.

| Artifact | Role |
|---|---|
| Agent | Owns posture, orchestration, judgment, delegation, and output contract. |
| Skill | Provides reusable workflow, domain knowledge, checklists, and validation criteria. |
| Hook | Enforces lightweight automated guardrails around session, prompts, tools, permissions, and completion. |
| Prompt template | Provides repeatable task framing for high-value workflows. |

## Validate

Run:

```bash
python3 .codex/scripts/validate_codex_tooling.py
```
