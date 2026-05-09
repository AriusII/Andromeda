# Repository-Read-Only Report-Only Profile

This profile means the worker may read repository files and run read-only commands. It must not mutate repository code or project assets.

The single exception is one Markdown report under `.work/codex/<task-slug>/analysis/`.

This profile exists because Codex needs a physical Markdown handoff artifact even when the worker is logically read-only.
