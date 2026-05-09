# Testing Strategy

## Purpose

Define the repository testing strategy without moving test ownership. Andromeda
uses risk-based gates: the closer a change is to durable truth, recovery,
security, protocol boundaries, or HA/DR, the stronger the required evidence.

## Ownership Model

| Test area | Owner location | Evidence expectation |
| --- | --- | --- |
| Unit and integration behavior | Owning crate tests and crate-local modules. | Exact command, result, commit, and artifact path. |
| Cross-crate roadmap labels | `tests/README.md` as an index only. | Compose owner-crate commands; do not create root harnesses only to satisfy labels. |
| Fuzz targets and corpora | `fuzz/` plus `tests/fuzzing/`. | Compile preflight for changed targets and sustained runs for promoted byte surfaces. |
| Miri | Owning crate commands or bounded subset tooling. | Undefined-behavior invariant, nightly version, unsupported operations, and result. |
| Loom | `tools/loom-models/` or owner-crate model targets. | Modeled state, bounds, safety property, command, and residual risk. |
| Release packet | `docs/testing/release-evidence-template.md`. | One retained record per command, skipped gate, drill, or manual decision. |

## Risk Classes

| Risk | Required test posture |
| --- | --- |
| Routine local change | Owner unit or integration tests plus formatting when applicable. |
| Parser, codec, persisted byte, or network byte change | Owner tests, golden vectors where available, fuzz compile preflight, and sustained fuzz before promotion. |
| Unsafe, aliasing, layout, FFI, or low-level buffer change | Owner tests plus Miri or accepted residual risk. |
| Locking, shutdown, publication ordering, backpressure, or concurrency change | Owner tests plus Loom or an explicit scope exclusion. |
| WAL, storage, recovery, MVCC, catalog publication, backup, restore, PITR, HA/DR, security, protocol, or audit change | Owner tests plus combined C4/C5 release gates with retained artifacts. |

## Baseline Development Gate

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --profile default --workspace --all-features --locked
cargo test --doc --workspace --all-features --locked
```

## C5 Gate Principles

- Visible commit must wait for durable WAL.
- Recovery truth comes from the latest valid durable snapshot or manifest plus
  durable WAL.
- Application traffic cannot invoke Administration, backup, restore, PITR,
  HA/DR, quorum, fencing, or forensic controls.
- Typed Procedure contracts, ContractHash rejection, authorization, and durable
  audit must be proven together for promoted execution paths.
- Advisory signals such as benchmark output, fuzz smoke, Miri inventory, or
  standalone Loom models remain partial until tied to the release scope.

## Troubleshooting

If a listed test moved, inspect the owning crate before editing docs. If a local
tool is missing, record the gate as blocked or attach retained CI evidence from a
configured host. If a command passes in isolation but the combined gate fails,
the combined failure controls the release decision.
