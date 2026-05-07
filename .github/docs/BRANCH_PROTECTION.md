# Branch Protection Guidance

Use `main` as the only long-lived branch. Use short-lived branches for pull requests when you want pre-merge enforcement.

Recommended ruleset for `main`:

- Require pull request before merge.
- Require status checks to pass.
- Require branches to be up to date before merge.
- Block force pushes.
- Block branch deletion.
- Require Code Owners review if future contributors are added.

Recommended required checks:

- `Cargo fast quality gate`
- `Andromeda project policy gate`
- `Protobuf without gRPC policy gate`
- `dependency-review`
- `cargo-audit`
- `cargo-deny`
- `actionlint`

The `Andromeda project policy gate` is a classified release gate. It blocks
active runtime/API drift in Cargo manifests, active Protobuf schemas, and
active Rust runtime sources. It does not fail on doctrine-negative references in
documentation, tests, skills, comments, or guardrail code; those references keep
the invariant visible without becoming release blockers.
