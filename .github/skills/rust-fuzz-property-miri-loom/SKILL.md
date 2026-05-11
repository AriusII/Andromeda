---
name: rust-fuzz-property-miri-loom
description: Apply fuzz property Miri and loom verification when parser codec unsafe concurrency or invariants keywords appear.
license: MIT
---

# rust-fuzz-property-miri-loom

## When to use
Use for SRPL parsers, codecs, canonical binary formats, unsafe code, concurrency, state machines, recovery invariants, fuzzing, proptest, Miri, or loom. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-fuzz-property-miri-loom` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Use advanced verification where example tests cannot cover adversarial inputs, undefined behavior, or scheduler interleavings. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Identify the invariant: parse/print, canonical bytes, reject invalid frames, no UB, no data race, or recovery convergence.
2. Choose the existing tool: property tests for broad domains, fuzz for parsers/codecs, Miri for unsafe/aliasing, loom for concurrency models.
3. Keep generators deterministic and shrinkable; seed corpora with spec examples and previous regressions.
4. Run only configured commands or document unavailable tools; do not add heavyweight infrastructure without a bounded need.
5. For large surfaces, dispatch `/agent` read-only workers to propose harnesses and consolidate before adding code.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/testing/FUZZING_PLAN.md`
- `docs/testing/PROPERTY_TEST_PLAN.md`
- `docs/adr/ADR-0003-UNSAFE_RUST_POLICY.md`
- `docs/adr/ADR-0004-CANONICAL_BINARY_FORMAT.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not fuzz live network or production resources.
- Do not accept nondeterministic harnesses.
- Do not treat fuzzing as a substitute for spec-based assertions.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
