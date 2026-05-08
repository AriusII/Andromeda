# Architecture Decision Records (ADRs)

This directory contains architecture decision records for the Andromeda project.

## Purpose

ADRs document significant architectural decisions, including:
- Context and problem statement
- Decision rationale and alternatives considered
- Consequences and implications
- Revision criteria (how we know this decision is still valid)

## Structure

Each ADR is a separate markdown file named `ADR-NNN-title.md` where NNN is a zero-padded number.

## Format

ADRs follow the standard format:
- **Status**: Proposed, Accepted, Deprecated, Superseded
- **Context**: Problem, constraints, background
- **Decision**: What was decided and why
- **Alternatives**: Other options considered
- **Consequences**: Positive and negative outcomes
- **Revision Criteria**: How to know if this decision still holds

## Examples

- ADR-001-rust-2024-edition.md
- ADR-002-quic-only-transport.md
- ADR-003-procedure-only-execution.md
- ADR-004-stable-channel-guarantee.md

## References

- [Sage and Michael Nygard's ADR Format](https://adr.github.io/)
- Andromeda invariants (see docs/architecture/)
