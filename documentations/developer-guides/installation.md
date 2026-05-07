# Installation Guide

## Codex project installation

Use the repository-provided `.codex` and `.agents` directories from the Andromeda root. Do not copy personal agent experiments into the project tree.

On Unix-like systems, make hook scripts executable if needed:

```bash
chmod +x .codex/scripts/hooks/*.py
```

Open Codex in the repository root:

```bash
cd /path/to/andromeda
codex
```

Verify hooks:

```text
/hooks
```

Ask Codex:

```text
List available agents and skills.
```

## OpenAI Agents SDK adaptation

Use `.codex/adapters/openai-agents/adapters/` as a starting point. The adapter is intentionally a sketch until it is wired to a real runtime context, policy engine, and audit sink. Map agent definitions to Python or TypeScript agent instances with:

- `instructions`
- `tools`
- `handoffDescription`
- `inputGuardrails`
- `outputGuardrails`
- lifecycle hooks
- tracing processors

## Repository policy

Commit only reviewed project-wide skills, agents, hooks, schemas, prompts, and workflows. Personal experiments should remain outside the repository.
