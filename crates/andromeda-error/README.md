# andromeda-error

## Purpose

`andromeda-error` defines the typed error vocabulary shared by Andromeda foundation crates.

Use this crate when code needs an `AndromedaErrorKind`, an `AndromedaError`, or the `AndromedaResult<T>` alias without depending on a higher-level engine crate.

## Scope

This crate owns:

- Stable, lower-case labels for broad engine error categories.
- A small typed error struct that preserves the category and message.
- The result alias used by low-level crates that must report typed failures.

The crate is an R0 foundation crate. It does not own storage recovery, catalog diagnostics, transport framing, retry policy, observability records, or user-facing localization.

## Non-goals

- Do not add application-facing SQL errors or SQL compatibility vocabulary.
- Do not model Procedure contracts, SRPL diagnostics, RPC status frames, audit events, or recovery reports here.
- Do not make free-form messages part of a durable or network byte contract.
- Do not add retry, timeout-budget, or transaction-state behavior beyond the typed error category.
- Do not depend on higher-level engine, runtime, benchmark, analytics, or GPU crates.

## Allowed Dependencies

`andromeda-error` has no workspace dependencies.

Allowed dependency policy:

- Use the Rust standard library only.
- Do not add dependencies on any Andromeda crate.
- Do not add external error frameworks unless an ADR and topology validation update approve the dependency.

## Invariants

- `unsafe` code is forbidden by `src/lib.rs`.
- Error categories must remain typed through `AndromedaErrorKind`.
- `AndromedaErrorKind::as_str()` labels are stable, lower-case identifiers.
- `AndromedaError` must preserve both the category and message.
- Persistent or network representations must use explicit codecs owned by the caller, not Rust native struct layout.
- New categories must stay broad enough for foundation use and narrow enough to avoid a generic catch-all bucket.

## Prerequisites

Before changing this crate:

1. Read `../../AGENTS.md`.
2. Read `../AGENTS.md`.
3. Check `../README.md` and `../../docs/adr/ADR-0011-workspace-crate-boundaries.md` for R0 dependency rules.
4. Inspect current tests in `src/error.rs`.

## Procedure

To use this crate:

1. Return `AndromedaResult<T>` from fallible foundation APIs.
2. Construct errors with the most specific existing `AndromedaErrorKind`.
3. Keep caller-owned context in the message string.
4. Convert the error to durable, network, or audit formats only through explicit owner codecs.

To extend this crate:

1. Add a category only when existing categories would hide a meaningful engine boundary.
2. Add or update tests that prove the stable label and display output.
3. Recheck dependency topology if `Cargo.toml` changes.

## Validation

Run the focused crate test after changing source or documentation that describes source behavior:

```powershell
cargo test -p andromeda-error
```

Run the topology guard if dependencies or crate-boundary text changes:

```powershell
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

## Troubleshooting

| Symptom | Action |
|---|---|
| A caller needs a subsystem-specific diagnostic. | Keep the broad category here and define detailed diagnostics in the owning subsystem crate. |
| A message needs to cross the network or become durable evidence. | Add an explicit codec in the protocol, storage, or observability owner. |
| A new error kind looks like a single call-site condition. | Keep it as caller context in the message instead of expanding the foundation vocabulary. |

## References

- `Cargo.toml`
- `src/lib.rs`
- `src/error.rs`
- `../README.md`
- `../../docs/adr/ADR-0011-workspace-crate-boundaries.md`
