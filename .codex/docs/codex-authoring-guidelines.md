# Codex Authoring Guidelines for Andromeda

## Agent design

Use agents for roles that require persistent operating behavior: orchestration, repository-read-only analysis, bounded write execution, roadmap coordination, architecture governance, and mission-critical review.

Do not create one agent per micro-topic. Put micro-methods into skills.

## Skill design

A skill must have one precise trigger and one repeatable method. It should include:

- YAML frontmatter with `name` and `description`.
- When to use.
- Purpose.
- Process.
- Expected output.
- Guardrails.

## Worker reports

Codex worker artifacts go under `.work/codex/<task-slug>/` and must be detailed enough for another worker to continue without guessing.
