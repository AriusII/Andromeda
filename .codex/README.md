# Andromeda Codex Operating Pack

`.codex` contains the active Codex-facing execution assets for Andromeda.

| Path | Purpose |
|---|---|
| `agents/` | Specialist agent definitions registered from `config.toml`. |
| `hooks.json` | Authoritative hook configuration. |
| `scripts/hooks/` | Hook implementations and policy gates. |
| `scripts/validate_codex_tooling.py` | Deterministic validation for agents, hooks, and skills. |
| `workflows/` | Repeatable Codex work procedures. |
| `templates/prompts/` | Reusable prompt templates. |
| `schemas/` | JSON schemas and local tooling schema assets. |
| `adapters/openai-agents/` | OpenAI Agents SDK adapter sketches. |
| `runtime/` | Local runtime output; do not commit generated logs. |

Use `.agents` for reusable skills, agent-facing standards, and governance registries.
