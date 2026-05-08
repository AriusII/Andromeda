# andromeda-catalog-recovery

## Purpose

`andromeda-catalog-recovery` owns dependency-light catalog recovery and publication
replay contract types that can be shared without depending on the live catalog owner.

## Scope

- Durable catalog mutation payload envelopes.
- Recovery anomaly and skipped-batch classifications.
- Administration/HA publication audience and reason classifications.
- Publication replay terminal and replay-record classifications.
- Compile-safe documentation of APIs that remain owned by `andromeda-catalog`.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No durable artifact replay orchestration.
- No catalog snapshot mutation.
- No WAL payload decoding.
- No publication receipt or subscriber registry validation.
- No WAL, page, or manifest protocol implementation in this scaffold.

## Prerequisites

- Rust 2024 toolchain aligned with the repository baseline.
- Existing recovery and catalog ownership rules must be defined by follow-up design work.

## Procedure

Use this crate for contract types that do not require `andromeda-catalog` ownership.
Keep APIs that require `CatalogSnapshot`, `CatalogMutationRecord`,
`CatalogPublicationReceipt`, `CatalogPublicationReport`, or subscriber registry state in
`andromeda-catalog` to avoid circular dependencies.

Current owner boundary:

| API group | Owner |
| --- | --- |
| Durable payload envelope and replay classifications | `andromeda-catalog-recovery` |
| Publication audience and replay classifications | `andromeda-catalog-recovery` |
| Snapshot recovery orchestration | `andromeda-catalog` |
| WAL payload decoding | `andromeda-catalog` |
| Publication report, receipt, and subscriber registry validation | `andromeda-catalog` |

## Validation

```powershell
cargo fmt --manifest-path crates/andromeda-catalog-recovery/Cargo.toml --check
cargo check --manifest-path crates/andromeda-catalog-recovery/Cargo.toml
cargo test --manifest-path crates/andromeda-catalog-recovery/Cargo.toml
cargo check -p andromeda-catalog -p andromeda-catalog-recovery --all-targets --all-features
```

## Troubleshooting

If validation reports workspace inheritance issues, verify the crate is still under
`crates/` and that this directory is being read from the repository root.

## References

- AGENTS instructions in repository root.
