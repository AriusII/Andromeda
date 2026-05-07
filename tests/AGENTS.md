# Test Instructions

These instructions apply to tests.

## Rules

- Prefer deterministic tests.
- Do not normalize flakiness through retries.
- Use property tests for codecs, canonicalization, and state machines.
- Use fuzz tests for untrusted or persisted byte inputs.
- Use crash/recovery tests for WAL, manifest, storage, catalog, and map consistency.
- Use Miri or sanitizer gates for unsafe code where feasible.
