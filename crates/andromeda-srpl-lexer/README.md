# SRPL Lexer Boundary Status

## Purpose

This file records the patchable status for extracting the SRPL lexer into
`andromeda-srpl-lexer`.

## Scope

The local crate files under `crates/andromeda-srpl-lexer/` contain the lexer
implementation and unit tests. The parser and SRPL facade have been adjusted to
import the lexer through the new crate boundary.

## Non-goals

This change does not alter SRPL grammar, parser behavior, binding, lowering,
catalog contracts, execution behavior, WAL behavior, or runtime procedure
visibility.

## Prerequisites

Resolve the concurrent root workspace manifest edits before applying the root
workspace patch. At the time this status file was written, `Cargo.toml` already
contained concurrent new workspace members such as:

- `crates/andromeda-codec`
- `crates/andromeda-maps`
- `crates/andromeda-policy`
- `crates/andromeda-procedure-store`
- `crates/andromeda-resource`

`cargo metadata` failed because at least `crates/andromeda-codec/Cargo.toml`
was missing.

## Procedure

The current root manifest contains `andromeda-srpl-lexer`. If that entry is
lost during conflict resolution, reapply this root-manifest patch:

```diff
diff --git a/Cargo.toml b/Cargo.toml
@@
     "crates/andromeda-srpl-diagnostics",
     "crates/andromeda-srpl-ir",
+    "crates/andromeda-srpl-lexer",
     "crates/andromeda-srpl-parser",
@@
 andromeda-srpl-diagnostics = { path = "crates/andromeda-srpl-diagnostics", version = "0.1.0" }
 andromeda-srpl-ir = { path = "crates/andromeda-srpl-ir", version = "0.1.0" }
+andromeda-srpl-lexer = { path = "crates/andromeda-srpl-lexer", version = "0.1.0" }
andromeda-srpl-parser = { path = "crates/andromeda-srpl-parser", version = "0.1.0" }
```

## Validation

Run these commands after the root workspace manifest loads successfully:

```bash
cargo fmt --package andromeda-srpl-lexer --package andromeda-srpl-parser --package andromeda-srpl --check
cargo test --package andromeda-srpl-lexer
cargo test --package andromeda-srpl-parser
cargo test --package andromeda-srpl --test api_compat_reexports
```

## Troubleshooting

If `cargo metadata` reports a missing workspace member, resolve that missing
crate or remove that unrelated member before validating this lexer boundary.

If `andromeda-srpl-lexer.workspace = true` cannot be resolved, confirm that the
root manifest includes the `andromeda-srpl-lexer` entry in
`[workspace.dependencies]`.

## References

- `crates/andromeda-srpl-lexer/src/lib.rs`
- `crates/andromeda-srpl-parser/src/lexer.rs`
- `crates/andromeda-srpl/src/lexer.rs`
