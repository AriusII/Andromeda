---
applyTo: "crates/**/tests/**/*.rs,crates/**/src/**/*test*.rs"
---

# Test instructions

- Tests live with the owning crate; do not add executable runtime tests under repository-root `tests/`.
- Prefer deterministic assertions and controlled fixtures. Do not hide flaky behavior with retries.
- Map test intent to nextest profiles: `default`, `ci`, `slow`, `recovery`, or `wal`.
- Validate WAL, recovery, fuzz/property, and crash-safety surfaces with evidence appropriate to their risk.
- Align with `docs/testing/TEST_STRATEGY.md` when adding or changing test strategy.
