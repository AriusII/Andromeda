# Installation Guide

## Claude Code project installation

Copy the `.claude` directory to the root of the Andromeda repository.

```bash
cp -R .claude /path/to/andromeda/
cp -R hooks /path/to/andromeda/
```

Make hook scripts executable:

```bash
chmod +x hooks/scripts/*.py
```

Open Claude Code in the repository root:

```bash
cd /path/to/andromeda
claude
```

Verify hooks:

```text
/hooks
```

Ask Claude Code:

```text
List available agents and skills.
```

## OpenAI Agents SDK adaptation

Use `openai-agents/adapters/` as a starting point. The agent and hook definitions are intentionally platform-neutral
Markdown plus YAML. Map them to Python or TypeScript agent instances with:

- `instructions`
- `tools`
- `handoffDescription`
- `inputGuardrails`
- `outputGuardrails`
- lifecycle hooks
- tracing processors

## Repository policy

Commit only project-wide skills, agents, hooks, schemas, prompts, and workflows that have been reviewed. Personal
experiments should remain outside the repository.
