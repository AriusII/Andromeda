# Andromeda Codex Rust Tooling Package

**Package version:** `2026.05.07-rust-codex-tooling-v2`  
**Language:** American English  
**Target:** Extract at the root of the `AriusII/Andromeda` repository.

## What this package provides

| Area | Count |
|---|---:|
| Codex agents | 72 |
| Codex skills | 107 |
| Hook scripts | 10 |
| Prompt templates | 30 |
| ADRs | 10 |

## Extraction

Extract this archive directly at the root of the Andromeda repository.

Expected root-level files and folders after extraction:

```text
AGENTS.md
.codex/
.agents/
docs/
crates/AGENTS.md
tests/AGENTS.md
README_CODEX_TOOLING_PACKAGE.md
```

## Validate

Run:

```bash
python3 .codex/scripts/validate_codex_tooling.py
```

## Design intent

This package is designed for Codex work on a modern 2026 Rust SGBDRT + SRPL project. It emphasizes:

- roadmap-following agents,
- context intake and prompt refinement agents,
- cleanup/refactor agents,
- Rust architecture and performance agents,
- Andromeda-specific mission-critical agents,
- many precise skills,
- hook guardrails,
- reusable prompt templates.

## Important

The package does not configure MCP servers. It is MCP-aware only. If `rust-lsp` or `rust-rover-ide` MCP tools are available in the running Codex environment, agents may use them. If not, agents must fall back to repository inspection and standard Rust tooling.
