# Official Codex Notes Used for This Package

## Purpose

Summarize the official and repository-observable Codex surfaces used to shape this package.

## AGENTS.md

The official Codex repository includes documentation for `AGENTS.md` and a hierarchical agents message behavior controlled by a `child_agents_md` feature flag.

## Configuration

The official Codex repository includes configuration documentation and references for basic, advanced, and full configuration. It also documents `commit_attribution` behavior behind the `codex_git_commit` feature flag.

## Skills

The official Codex skill creator sample defines a skill as a self-contained folder with:

- required `SKILL.md`,
- YAML frontmatter with `name` and `description`,
- optional `scripts/`,
- optional `references/`,
- optional `assets/`,
- optional UI metadata.

The package follows that model.

## Hooks

The official Codex source and tests show command hooks for:

- `SessionStart`
- `UserPromptSubmit`
- `PreToolUse`
- `PostToolUse`
- `PermissionRequest`
- `Stop`

The package configures all of these events and uses explicit `timeout` and `statusMessage` values.

## External agent migration

The official Codex source includes migration support for external agent skills, subagents, hooks, commands, MCP server config, and AGENTS.md. This package uses the same target directories:

```text
AGENTS.md
.codex/config.toml
.codex/agents
.codex/hooks.json
.agents/skills
```
