---
name: unsafe-rust-audit
description: "Audit unsafe Rust, binary layout, FFI, direct I/O, mmap, pointer arithmetic, lock-free structures, and safe wrapper invariants. Use when working on rust tasks that require this workflow."
---

# Unsafe Rust Audit

## Purpose

Use this skill to perform a focused Andromeda workflow without re-deriving the project doctrine from scratch. Keep the output precise, testable, and aligned with the current consolidated Andromeda documents.

## Core workflow

1. Identify the exact object under review: document section, Procedure, contract, module, test, hook, skill, agent, or source file.
2. Restate the relevant Andromeda invariants before proposing changes.
3. Separate stable doctrine from open design decisions.
4. Produce a concrete artifact: review notes, a patch plan, a specification, a test matrix, a checklist, or a corrected document section.
5. State validation commands or deterministic checks that should be run.
6. List residual risks and the smallest next action.

## Andromeda constraints to preserve

- No ad hoc SQL application surface.
- Procedure-only RPC boundary.
- Typed, hashed, versioned Procedure contracts.
- Explicit transaction scope and isolation policy where relevant.
- No visible commit without durable WAL.
- Recovery and audit must be designed before critical features are accepted.
- GPU, learned components, and predictive evidence may assist but must not become C5 truth.
- Any adaptive behavior must be bounded, versioned, observable, explainable, and disableable.

## Output format

```markdown
## Scope
<what was reviewed or designed>

## Findings
- [Critical|High|Medium|Low] <finding>

## Recommended changes
1. <specific change>

## Validation
- <command or test>

## Residual risks
- <risk>
```

## When to read references

Read `references/checklist.md` when the task is a formal review, acceptance gate, or forward-test. Do not load it for trivial edits.
