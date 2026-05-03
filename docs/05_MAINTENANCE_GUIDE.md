# Maintenance Guide

## Versioning

Every instruction, agent, skill, hook, workflow, and prompt template must declare a version. Use semantic versioning:

- Patch: wording clarification without behavior change.
- Minor: new checks, new examples, new optional procedures.
- Major: changed behavior, stricter gate, renamed outputs, or incompatible frontmatter.

## Review cadence

Review this pack after:

- A new Andromeda architectural decision.
- A new SRPL syntax decision.
- A new QUIC/Protobuf contract rule.
- A new repository layout.
- A new Rust toolchain or test strategy.
- A new AI platform lifecycle or security capability.

## Deprecation

Do not delete agents or skills immediately. Mark as deprecated, explain the replacement, and remove after one release
cycle.

## Testing

Test skills with representative prompts. Test hooks with JSON input fixtures. Test agent handoffs with at least one
successful and one rejected scenario.
