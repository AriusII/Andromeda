# andromeda-srpl-diagnostics

## Purpose

`andromeda-srpl-diagnostics` owns parser-independent SRPL source spans,
diagnostic phases, forbidden construct diagnostics, and source validation
helpers.

Use this crate when code needs source-location primitives or deterministic
diagnostics before parser, binder, lowering, or execution-specific handling.

Diagnostics are part of the SRPL safety boundary. They reject forbidden core
constructs before ad hoc text, nondeterminism, external effects, or unbounded
control flow can enter the compiler pipeline.

## Scope

This crate is responsible for:

| Area | Responsibility |
| --- | --- |
| Source spans | `SourceSpan` byte ranges and validation helpers. |
| Source wrapper | `SrplSource` validation over raw source text. |
| Diagnostic phases | Phase labels for lexing, parsing, binding, semantic validation, IR lowering, plan candidate generation, and runtime binding. |
| Forbidden constructs | Deterministic diagnostics for unbounded loops, free recursion, external network and filesystem access, nondeterministic random calls, dynamic text SQL, and `select *`. |

The crate is parser-independent and runtime-free. It does not own parsing,
binding, lowering, execution, catalog storage, transport, benchmarks, analytics,
or GPU work.

## Non-goals

- Do not parse SRPL grammar, bind catalog objects, lower IR, optimize, execute,
  or materialize catalog definitions.
- Do not introduce application-facing ad hoc SQL.
- Do not normalize dynamic table names, dynamic predicates, shape-shifting returns,
  or implicit null semantics into accepted SRPL.
- Do not use diagnostics as durable storage, network payloads, or catalog truth.
- Do not serialize Rust native structs directly to disk or network.
- Do not depend on parser, catalog store, execution, storage, transport runtime,
  benchmark, analytics, or GPU crates.

## Prerequisites

Before changing this crate, understand:

- `SourceSpan` uses byte offsets. Spans must remain ordered, bounded, and valid
  for UTF-8 source slicing.
- Forbidden construct detection is a front-line safety check, not the only
  semantic validation stage.
- `DynamicTextSql` covers text-SQL construction attempts. SRPL core still also
  rejects dynamic table names, dynamic predicates, shape-shifting returns, and
  implicit null semantics through parser, binder, and IR constraints.
- Diagnostics should be deterministic and actionable, with stable phase
  attribution.

## Procedure

1. Keep source-span helpers independent from parser internals.
2. Add forbidden construct diagnostics with stable diagnostic codes and clear
   phase ownership.
3. Preserve UTF-8-safe byte span behavior when scanning source text.
4. Avoid substring-only rules that create false positives for identifiers.
5. Return typed SRPL errors through project error types when validation fails.
6. Add tests for positive matches, false positives, case and whitespace
   variants, and UTF-8 span safety.

## Validation

For documentation-only changes, validate this README against:

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`

For code changes in this crate, prefer:

```bash
cargo fmt --package andromeda-srpl-diagnostics --check
cargo test -p andromeda-srpl-diagnostics
cargo check -p andromeda-srpl-diagnostics --all-targets
```

If a forbidden construct changes accepted SRPL source, also run parser,
compiler compatibility surface, and validation-gate tests.

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| A diagnostic span splits a UTF-8 scalar. | Fix scanning to emit byte ranges on character boundaries and add a UTF-8 regression test. |
| A valid identifier triggers a forbidden construct. | Replace substring detection with token-like structural matching. |
| Dynamic text SQL is accepted by source validation. | Add or fix a `DynamicTextSql` detection path and downstream rejection test. |
| A diagnostic tries to execute or inspect catalog state. | Move that behavior to binder, lowering, catalog, or execution code. Diagnostics stay source-local. |
| A nullable value is accepted implicitly. | Reject it in parser or binder and report through explicit SRPL diagnostics. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `crates/andromeda-srpl-diagnostics/src/lib.rs`
- `crates/andromeda-srpl-diagnostics/src/diagnostics.rs`
- `crates/andromeda-srpl-diagnostics/src/source_location.rs`
- `crates/andromeda-srpl-diagnostics/src/source_location/forbidden_scan.rs`
