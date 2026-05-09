---
name: codex-skill-authoring
description: "Use to author precise Codex skills with strong triggers and guardrails."
category: codex-tooling
---

# codex-skill-authoring

## When to use
Need to create or improve .codex/skills/*/SKILL.md.

## Purpose
Use to author precise Codex skills with strong triggers and guardrails.

## Process
- Use YAML frontmatter with `name` and `description`.
- Description must say when to use the skill.
- Keep each skill focused on one repeatable method.
- Use progressive disclosure: core method in SKILL.md, long references in separate files if needed.
- Include trigger, process, output contract, guardrails, and validation.

## Expected output
- Valid SKILL.md with frontmatter and operational content.

## Guardrails
- Do not create vague skills. Do not duplicate entire agent instructions.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
