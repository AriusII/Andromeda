# andromeda-security

## Purpose

`andromeda-security` reserves a future security domain boundary.

## Scope

- Own scaffold-only security crate identity.
- Keep dependencies and behavior empty until a concrete security responsibility is assigned.
- Keep existing security contract vocabulary in `andromeda-security-contract` and pre-transaction admission behavior in `andromeda-iam`.
- Keep durable IAM state, runtime storage integration, WAL coupling, and transaction-time security enforcement out of this crate.

## Non-goals

- Do not add active policy enforcement, authentication, authorization logic, runtime dependencies, or persistent integrations while this crate remains scaffold-only.
- Do not duplicate `andromeda-security-contract` or `andromeda-iam` ownership.
- Do not expose application-facing ad hoc SQL or administration bypass behavior.

## Prerequisites

- Workspace conventions from `AGENTS.md` and `crates/AGENTS.md` are in force.
- Security ownership boundaries should be finalized before implementation is added.

## Procedure

1. Keep `Cargo.toml` workspace-style and dependency-free.
2. Keep `src/lib.rs` minimal with `#![forbid(unsafe_code)]`.
3. Document any future security ownership before adding behavior.

## Validation

- Inspect `Cargo.toml`, `README.md`, and `src/lib.rs` for scaffold-only scope.

## Troubleshooting

- If behavior appears here without an ownership decision, move it to the current security owner.
- If dependency pressure appears, identify whether the dependency belongs to contract vocabulary, IAM runtime, or audit evidence instead.

## References

- `AGENTS.md`
- `crates/README.md`
- `crates/andromeda-security-contract/README.md`
- `crates/andromeda-iam/README.md`
