#!/usr/bin/env python3
from common import emit, event_context, read_payload

payload = read_payload()
emit(event_context("PostToolUse", "Codex tooling audit reminder: agents, skills, hooks, and prompts must remain distinct. Validate registry and frontmatter before packaging."))
