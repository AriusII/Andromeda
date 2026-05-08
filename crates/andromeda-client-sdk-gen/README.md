# andromeda-client-sdk-gen

## Purpose

`andromeda-client-sdk-gen` reserves the runtime-free boundary for future client SDK generation.

## Scope

- Own SDK generation boundary markers.
- Keep future SDK inputs explicit, versioned, and contract-derived.
- Keep generated client artifacts separate from server execution and transport runtime behavior.

## Non-goals

- Do not own protocol transport, wire codecs, server-side Procedure execution, catalog state, WAL, or recovery.
- Do not treat generated artifacts, templates, or fixtures as production runtime truth.
- Do not emit stable SDK bindings before the source contracts and protocol inputs are defined.

## Prerequisites

- Contract inputs come from the owning Procedure, RPC, and protocol crates.
- Generation behavior remains deterministic before it is wired to tools or CI.

## Procedure

1. Keep generation APIs isolated from runtime transport and server execution.
2. Version every accepted input contract before generating artifacts from it.
3. Keep CLI or packaging integration outside this crate until the generation contract is stable.

## Validation

- Inspect the crate for runtime-free marker scope.
- When implementation is added, run `cargo check -p andromeda-client-sdk-gen --all-targets`.

## Troubleshooting

- If generated output drifts, lock the input contract and template version.
- If the source contract is unstable, defer SDK generation rather than inventing a client shape.

## References

- `AGENTS.md`
- `crates/README.md`
- `crates/andromeda-rpc/README.md`
- `crates/andromeda-procedure-contract/README.md`
