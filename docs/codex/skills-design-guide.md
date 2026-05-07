# Skills Design Guide

## Purpose

Define how Andromeda skills are designed.

## Rules

- A skill is not an agent.
- A skill must be reusable.
- A skill must have a precise trigger description.
- A skill should include only the minimum body required to execute.
- Detailed checklists belong in `references/checklist.md`.
- Scripts belong in `scripts/` only when deterministic execution is useful.
- Skills must not duplicate the same behavior across many folders.

## Skill structure

```text
skill-name/
  SKILL.md
  references/
    checklist.md
  agents/
    openai.yaml
```

## Frontmatter

Use:

```yaml
---
name: skill-name
description: "What this skill does and exactly when to use it."
---
```

## Validation

Run:

```bash
python3 .codex/scripts/validate_codex_tooling.py
```
