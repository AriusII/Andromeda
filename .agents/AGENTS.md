# Skills Library Instructions

This directory contains Codex skills.

## Rules

- Every skill must be a folder named with lowercase letters, digits, and hyphens.
- Every skill must contain `SKILL.md`.
- Every `SKILL.md` must have YAML frontmatter with only `name` and `description` unless a future Codex standard requires otherwise.
- The `name` field must match the folder name.
- The description must include what the skill does and when to use it.
- Keep SKILL.md concise. Move detailed checklists and examples to `references/`.
- Do not duplicate an agent role inside a skill. A skill is a reusable workflow, not an autonomous actor.
