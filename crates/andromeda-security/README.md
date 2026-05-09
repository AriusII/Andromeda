# andromeda-security

## Purpose

`andromeda-security` owns dispatch-time security authorization that is above
runtime-free security vocabulary and below observe sinks/envelopes.

## Scope

- Own the V0 principal-binding registry and surface authorizer used before
  procedure dispatch.
- Own the procedure-dispatch capability token that proves an Application-surface
  `ExecuteProcedure` decision happened before local transaction creation.
- Keep existing security contract vocabulary in `andromeda-security-contract` and pre-transaction admission behavior in `andromeda-iam`.
- Keep durable IAM state, runtime storage integration, WAL coupling, and transaction-time security enforcement out of this crate.
- Emit audit DTOs owned by `andromeda-audit`; do not depend on
  `andromeda-observe`.

## Non-goals

- Do not own observe envelopes, sinks, query, or exporter behavior.
- Do not duplicate `andromeda-security-contract` runtime-free vocabulary or
  `andromeda-iam` pre-transaction admission ownership.
- Do not expose application-facing ad hoc SQL or administration bypass behavior.

## Prerequisites

- Workspace conventions from `AGENTS.md` and `crates/AGENTS.md` are in force.
- Security owns only the observe-independent principal-binding authority in this
  crate; observe may keep compatibility reexports during caller migration.

## Procedure

1. Keep `Cargo.toml` workspace-style and limited to audit payloads,
   observability IDs, and foundation errors.
2. Keep `src/lib.rs` explicit about exported security authority types.
3. Do not add an `andromeda-observe` dependency; observe sits above this crate.

## Validation

- `cargo check -p andromeda-security --all-targets --all-features`
- `cargo test -p andromeda-security --all-targets --all-features`

## Troubleshooting

- If an observe dependency appears, move envelope/query/exporter behavior back to
  observe or project through audit/observability DTOs instead.
- If durable IAM state or admission policy pressure appears, identify whether
  the behavior belongs in `andromeda-iam` or `andromeda-principal`.

## References

- `AGENTS.md`
- `crates/README.md`
- `crates/andromeda-security-contract/README.md`
- `crates/andromeda-iam/README.md`
