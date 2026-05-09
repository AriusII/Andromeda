# andromeda-srpl-test-fixtures

## Purpose

`andromeda-srpl-test-fixtures` owns shared SRPL test fixtures.

## Scope

- Centralize SRPL input/output fixture data for tests.
- Keep test artifact ownership separated from production runtime and protocol crates.
- Define deterministic fixture shapes for parser, binder, adapter, and interpreter tests.

## Non-goals

- Implement parser, binder, or interpreter logic.
- Introduce production dependencies or runtime side effects.
- Define executable SQL text contracts or untyped payload interfaces.

## Prerequisites

- Rust 2024 workspace baseline.
- Existing SRPL test patterns and fixture naming conventions.

## Procedure

1. Add fixture modules only when reusable test payloads are identified.
2. Keep fixture construction deterministic and version-stable.
3. Keep public API narrow to avoid leaking test internals to production paths.

## Validation

- `cargo check -p andromeda-srpl-test-fixtures --all-targets`
- Keep fixtures deterministic and free of production side effects.

## Troubleshooting

- If fixture ownership conflicts with another crate, consolidate ownership before implementation.
- Keep fixture and test helpers behind test-only boundaries when possible.

## References

- `AGENTS.md`
- `crates/AGENTS.md`
