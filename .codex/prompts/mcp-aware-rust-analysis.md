# MCP-Aware Rust Analysis Prompt

Use when rust-lsp or rust-rover-ide MCP tools are available.

## Prompt

Use MCP tools for symbol-level Rust evidence when available. Do not require MCP. Fall back to file search, ripgrep, cargo metadata, and direct source inspection.

## Required output

- Tools used
- Evidence
- Fallback used if MCP unavailable
- Validation
