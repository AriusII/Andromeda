# Mission-Critical Rust Review Prompt

Review Rust code as if it may enter a C4/C5 Andromeda subsystem.

## Required checks

- No panic path in runtime-critical behavior
- No unreviewed unsafe
- No native struct persistence
- Typed errors
- Recovery implications
- Observability
- Tests
