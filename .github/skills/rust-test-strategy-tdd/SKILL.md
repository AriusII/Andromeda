---
name: rust-test-strategy-tdd
description: Design Rust tests and TDD plans when test strategy TDD risk class property recovery or fixture keywords appear.
license: MIT
---

# rust-test-strategy-tdd

## When to use
Use for new tests, TDD, acceptance criteria, risk-class mapping, integration/unit boundaries, fixtures, recovery tests, property tests, and regression coverage. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-test-strategy-tdd` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Create deterministic tests owned by the responsible crate and scaled to the risk of the behavior being changed. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Start with failing or characterization tests before implementation when behavior is unclear.
2. Map tests to risk: unit for pure logic, integration for crate-owned paths, property/fuzz for parsers/codecs, crash-recovery for WAL/storage, and security matrices for admission.
3. Place executable runtime tests with owning crates, not root `tests/`, unless repository policy already assigns that owner.
4. Run the narrow command for the test target, then the canonical gates when feasible.
5. Use `/agent` read-only test planners for complex features, and include exact cargo commands in worker missions.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/testing/TEST_STRATEGY.md`
- `docs/testing/PROPERTY_TEST_PLAN.md`
- `docs/testing/CRASH_RECOVERY_TEST_PLAN.md`
- `tests/README.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not hide flakiness with retries.
- Do not over-mock durability/security behavior.
- Do not claim coverage without running or explaining skipped commands.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
