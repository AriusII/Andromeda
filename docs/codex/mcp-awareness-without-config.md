# MCP Awareness Without MCP Configuration

## Purpose

Document how Codex should account for `rust-lsp` and `rust-rover-ide` MCP servers without requiring them in this package.

## Rule

This package does not configure MCP servers. It only tells agents and skills to use MCP tools when they are available.

## Expected behavior

When MCP tools are available:

- Use them for Rust symbol lookup.
- Use them for diagnostics.
- Use them for workspace navigation.
- Use them to reduce blind text search.

When MCP tools are unavailable:

- Use repository search.
- Use `cargo metadata`.
- Use direct source inspection.
- Use `ripgrep` or equivalent.
- State the fallback used.

## Rationale

The user has MCP servers in the broader environment, but this package must remain usable without those servers.
