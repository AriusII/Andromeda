# Andromeda Codex Operating Instructions

**Status:** Mission-critical Codex guidance  
**Package version:** 2026.05.07-rust-codex-tooling-v2  
**Project:** Andromeda - modern relational transactional database engine and SRPL language  
**Primary implementation language:** Rust 2024 Edition  
**Default engineering posture:** strict boundaries, adaptive internals

## Purpose

Use this file as the root instruction set for Codex when working in the Andromeda repository.

Andromeda is not a general SQL server with a modern wrapper. It is a strict relational transactional database engine where application behavior is exposed through typed, cataloged Procedures invoked through a contract-first RPC surface. Treat every design or code change as part of a safety-critical engine unless the task explicitly says it is experimental.

## Non-negotiable project invariants

1. Do not introduce application-facing ad hoc SQL.
2. Do not bypass typed Procedure contracts.
3. Do not make a commit visible before durable WAL.
4. Do not treat RAM, temp storage, GPU output, or benchmark output as truth.
5. Do not place GPU work in the commit, WAL, rollback, recovery, MVCC short-visibility, catalog publication, or security-critical path.
6. Do not serialize Rust native structs directly to disk or network.
7. Do not use dynamic table names, dynamic predicates, shape-shifting returns, or implicit null semantics in SRPL core work.
8. Do not expose Administration or HA/DR capabilities through the Application Surface.
9. Do not accept optimization that is not observable, bounded, versioned, explainable, and disableable.
10. Do not accept mission-critical behavior without crash/recovery validation.

## Rust baseline

Use Rust 2024 Edition unless an Architecture Decision Record changes the baseline.

Default Rust engineering rules:

- Keep crates aligned with engine responsibilities.
- Keep `lib.rs` thin.
- Prefer newtypes over primitive aliases for semantic identifiers.
- Keep `unsafe` private, documented, tested, and reviewed.
- Prefer typed errors over string errors.
- Do not use `panic`, `unwrap`, or `expect` in critical runtime paths.
- Use explicit binary codecs and little-endian canonical serialization.
- Use scalar fallbacks for CPU SIMD and GPU paths.
- Use measurement before optimization.

## Codex behavior

When a user request is broad, perform these steps:

1. Normalize the request into a concrete work order.
2. Identify the Andromeda subsystem and risk class.
3. Select the relevant agent and skills from `.codex/agents` and `.agents/skills`.
4. Preserve source evidence and uncertainty.
5. Execute the smallest safe slice.
6. Validate with the strongest reasonable gate.
7. Report what changed, what was validated, and what remains risky.

## Repository tooling

Use:

- `.codex/config.toml` for agent registry and Codex configuration.
- `.codex/agents/*.toml` for specialized agents.
- `.agents/skills/*/SKILL.md` for specialized skills.
- `.codex/hooks.json` and `.codex/scripts/hooks/*.py` for hook policies.
- `.codex/scripts/validate_codex_tooling.py` to validate this tooling package.
- `.codex/prompts/*.md` for reusable prompt templates.
- `docs/codex/` for operating model and governance documentation.

## Required validation before completing Codex tooling changes

Run:

```bash
python3 .codex/scripts/validate_codex_tooling.py
```

For Rust code changes, prefer the applicable gate:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
cargo test --doc --workspace
cargo audit
cargo deny check
```

For C4/C5 storage, WAL, recovery, security, RPC, or catalog changes, add crash/recovery, fuzz, Miri, or targeted property tests as appropriate.

## Output style

Use American English for generated project files and documentation.

Use Microsoft Learn-style structure:

- Purpose
- Scope
- Non-goals
- Prerequisites
- Procedure
- Validation
- Troubleshooting
- References

For chat responses to the repository owner, explain in French unless the user requests otherwise.
