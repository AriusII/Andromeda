# Codex Tooling Instructions

This directory contains Codex configuration, agents, hooks, scripts, prompt templates, and routing metadata.

## Rules

- Do not create unregistered agents.
- Do not create hooks without explicit `timeout` and `statusMessage`.
- Do not make hooks depend on secrets, network access, or non-standard system tools.
- Do not create agents that duplicate skills. Agents decide and orchestrate. Skills provide reusable procedures and domain knowledge.
- Keep read-only auditors in `sandbox_mode = "read-only"`.
- Keep implementation agents in `sandbox_mode = "workspace-write"` unless the task is inherently read-only.
- Validate the tree with `.codex/scripts/validate_codex_tooling.py`.

## Recommended change flow

1. Add or update skills first.
2. Add or update agent primary skill lists.
3. Register agents in `.codex/config.toml`.
4. Update routing metadata.
5. Validate hooks and scripts.
6. Update documentation and prompts.
