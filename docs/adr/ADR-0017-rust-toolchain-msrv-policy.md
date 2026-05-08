# ADR-0017: Rust Toolchain And MSRV Policy

## Status

Accepted

## Purpose

Record the exact Rust toolchain and minimum supported Rust version policy for
Andromeda release validation.

ADR-0005 remains authoritative for the Rust 2024 Edition baseline. This ADR
adds the concrete release-validation toolchain, dependency MSRV drift policy,
lockfile update policy, and evidence required before the workspace baseline can
be changed again.

## Scope

This ADR applies to:

- the root workspace Rust edition and `rust-version`;
- `rust-toolchain.toml`;
- dependency additions, removals, feature changes, and updates;
- `Cargo.lock` refreshes and lockfile review;
- CI and release validation that claims Rust 1.95.0 compatibility;
- future proposals to raise the workspace MSRV.

## Non-goals

This ADR does not:

- change ADR-0005 or the Rust 2024 Edition decision;
- approve the current dependency graph as release-ready evidence by itself;
- require nightly Rust;
- allow application-facing ad hoc SQL, native Rust struct serialization, or any
  other project invariant bypass;
- replace supply-chain, crash/recovery, fuzz, Miri, Loom, or security gates
  required by the owning runtime change.

## Prerequisites

Use these sources when applying this policy:

- `AGENTS.md` sets Rust 2024 Edition as the implementation baseline unless an
  ADR changes it.
- `docs/adr/ADR-0005-rust-2024-baseline.md` accepts Rust 2024 Edition and a
  modern stable Rust policy.
- `Cargo.toml` currently sets `[workspace.package] edition = "2024"` and
  `rust-version = "1.95.0"`.
- `rust-toolchain.toml` currently pins `channel = "1.95.0"` with `clippy` and
  `rustfmt` components.
- `documentations/governance/msrv-dependency-risk-2026-05-08.md` records the
  retired Rust 1.85 compatibility risk and the active Rust 1.95.0 dependency
  evidence requirement.
- `documentations/governance/supply-chain-policy.md` defines dependency
  admission and supply-chain gates.
- `documentations/governance/release-readiness-gates-2026-05-08.md` blocks
  release readiness until Rust 1.95.0 and dependency compatibility are validated
  with retained evidence.

## Decision

Andromeda's current release-validation baseline is:

| Field | Policy |
| --- | --- |
| Rust edition | Rust 2024 Edition, as accepted by ADR-0005. |
| Workspace MSRV | `rust-version = "1.95.0"` in the root `Cargo.toml`. |
| Pinned release toolchain | `channel = "1.95.0"` in `rust-toolchain.toml`. |
| Required components | `clippy` and `rustfmt`. |
| Compatibility claim | A release that claims current baseline compatibility must pass the applicable gates under Rust 1.95.0 with `--locked`. |

Developers must record the exact compiler used for any evidence claim. Local
success under any compiler other than the pinned toolchain is advisory until the
release owner accepts it for the stated scope. Release evidence must record the
exact `rustc` and `cargo` versions, command lines, target or feature scope,
pass/fail result, artifact path when applicable, and residual risk.

## Dependency MSRV Drift

Dependency MSRV drift is release-blocking for any release that claims
compatibility with Rust 1.95.0.

Any dependency addition, update, removal, feature change, or lockfile refresh
must be reviewed against the current baseline. If Cargo metadata, crate
metadata, build output, or a supply-chain tool reports a dependency requiring a
Rust version newer than 1.95.0, the change is blocked until one of these outcomes
is accepted:

- pin, downgrade, replace, or configure the dependency so every release-critical
  target path remains Rust 1.95.0-compatible;
- isolate the dependency outside the claimed release target, platform, feature,
  and artifact scope, and document the proof;
- raise the workspace MSRV through a later accepted decision record and the
  evidence listed in this ADR.

`Cargo.lock` is not sufficient by itself unless it contains reliable
package-level MSRV metadata for the dependency set under review. If lockfile
metadata is absent or incomplete, record the result as inconclusive, not as a
pass, and use richer evidence such as `cargo metadata`, registry metadata, or a
Rust 1.95.0 build gate.

## Lockfile Update Policy

`Cargo.lock` is release evidence and must not drift opportunistically.

Lockfile updates are allowed only as part of:

- an owned dependency admission, update, removal, or feature-change task;
- an explicit lockfile refresh task with a named owner and rationale;
- a release-candidate stabilization task that records the exact command and
  dependency diff.

Prefer targeted lockfile updates, such as `cargo update -p <package>` or
`cargo update -p <package> --precise <version>`, when the intent is package
specific. A broad `cargo update` requires supply-chain owner review, a rationale
for accepting the full dependency graph change, and renewed MSRV,
advisory, license, duplicate, build, and test evidence.

After a lockfile update, validation must use `--locked` so later commands prove
the committed lockfile, not a fresh resolver result. A stale lockfile or a
dependency that requires a newer compiler must be resolved in the owning
dependency or release-governance task. Do not repair lockfile drift inside an
unrelated documentation-only, formatting-only, or runtime feature task.

## Required Evidence Before Raising MSRV

The workspace MSRV may be raised only by an accepted decision record or an
accepted amendment to this ADR. The proposal must include:

- the proposed new `rust-version`, exact toolchain channel, and required
  components;
- the reason the current baseline is no longer sufficient, such as a required compiler
  feature, security fix, dependency MSRV, platform requirement, or toolchain
  bug;
- evidence that keeping the current baseline by pinning, downgrading, replacing, or
  isolating dependencies was considered;
- a dependency graph review that identifies packages and reverse dependency
  paths that force or benefit from the raise;
- target, platform, and feature-scope impact, including any release artifacts
  that would stop claiming current-baseline compatibility;
- updated CI and release gate commands that use the proposed toolchain with
  `--locked` where Cargo supports it;
- full workspace evidence under the proposed toolchain:

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --workspace --all-features --locked
cargo test --doc --workspace --locked
cargo audit --deny warnings
cargo deny check --all-features
```

- targeted crash/recovery, fuzz, Miri, Loom, security, RPC, catalog, storage,
  WAL, transaction, and backup/restore evidence when the compiler or dependency
  change can affect those surfaces;
- retained evidence that the previous baseline either fails for the intended scope or is
  intentionally retired by release governance;
- documentation updates for `Cargo.toml`, `rust-toolchain.toml`, release gates,
  risk register, dependency risk evidence, and any affected ADR or DEC index.

## Procedure

Use this procedure for Rust toolchain or dependency review.

1. Confirm the active edition and MSRV in the root `Cargo.toml`.
2. Confirm the pinned toolchain in `rust-toolchain.toml`.
3. For dependency changes, classify each changed dependency as runtime,
   development-only, build-only, or tooling-only.
4. Determine whether the dependency can affect C5 durability, recovery,
   security, RPC, catalog publication, persistent bytes, or network bytes.
5. Check dependency MSRV evidence from available metadata and a Rust 1.95.0 build
   gate when release compatibility is claimed.
6. Update `Cargo.lock` only in the owning dependency or lockfile task.
7. Rerun the applicable commands with `--locked`.
8. Record pass, fail, blocked, skipped, or inconclusive results. Do not convert
   inconclusive MSRV evidence into a pass.
9. Escalate to release governance before raising `rust-version`, changing the
   pinned toolchain, or accepting a newer-MSRV dependency in a release-critical
   path.

## Alternatives Considered

| Alternative                                                                          | Decision | Reason |
|--------------------------------------------------------------------------------------| --- | --- |
| Keep only ADR-0005 and no exact MSRV policy.                                         | Rejected. | It leaves ambiguity between Rust 2024 Edition, local stable compilers, and release-validation compatibility. |
| Track latest stable Rust for release validation.                                     | Rejected. | Mission-critical release evidence must be reproducible and must not silently change when a new compiler is published. |
| Keep the prior Rust 1.95 baseline after dependency and toolchain drift was observed. | Rejected. | The active workspace now validates under the pinned Rust 1.95.0 toolchain and no longer claims Rust 1.85 compatibility. |
| Allow broad lockfile updates during unrelated work.                                  | Rejected. | Broad resolver changes can alter MSRV, licenses, advisories, duplicates, and platform-specific dependency paths outside the owned task. |
| Use nightly Rust for release validation.                                             | Rejected. | The project baseline is stable Rust unless a later ADR records a narrower exception. |

## Consequences

The project has a concrete release-validation baseline: Rust 2024 Edition,
workspace `rust-version = "1.95.0"`, and pinned toolchain `1.95.0`.

Dependency updates that require a newer compiler are treated as governance
events, not routine resolver churn. This can require pins, downgrades,
replacements, feature isolation, or a formal MSRV raise.

Lockfile changes become auditable release evidence. They require an owner,
rationale, targeted commands where possible, and renewed `--locked` validation.

## Validation

This ADR is a documentation-only decision. It was validated by targeted source
inspection of:

- `AGENTS.md`;
- `docs/AGENTS.md`;
- `docs/adr/ADR-0005-rust-2024-baseline.md`;
- `Cargo.toml`;
- `rust-toolchain.toml`;
- `documentations/governance/msrv-dependency-risk-2026-05-08.md`;
- `documentations/governance/supply-chain-policy.md`;
- `documentations/governance/release-readiness-gates-2026-05-08.md`;
- `documentations/governance/adr-backlog-2026-05-08.md`.

No Rust build, Cargo test, lockfile update, dependency update, or tooling change
is approved by this ADR alone.

## Risks

- Developers can accidentally rely on compiler behavior outside the pinned
  toolchain unless CI and release evidence keep Rust 1.95.0 gates active.
- Dependency metadata can be incomplete or target-specific, so lockfile-only
  evidence can understate MSRV drift.
- A broad lockfile update can introduce newer compiler requirements even when
  first-party manifests still say `rust-version = "1.95.0"`.
- Raising MSRV without retained evidence can invalidate downstream release
  claims, reproducibility assumptions, or platform support.

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| `cargo check --locked` fails because a dependency requires Rust newer than 1.95.0. | Pin, downgrade, replace, or isolate the dependency, or raise MSRV through an accepted decision record with retained evidence. |
| `Cargo.lock` has no package-level MSRV metadata. | Record lockfile evidence as inconclusive and use `cargo metadata`, registry metadata, or a Rust 1.95.0 build gate. |
| A dependency update changes many unrelated packages. | Require owner review, rationale, dependency diff inspection, and renewed MSRV, audit, deny, build, and test evidence. |
| A document claims latest stable Rust is the release baseline. | Replace the claim with Rust 1.95.0 and the pinned `1.95.0` release-validation toolchain unless a later ADR changes the baseline. |
| A proposal raises `rust-version` without showing why the current baseline is insufficient. | Reject the raise until the proposal includes dependency paths, alternatives considered, gate updates, and retained validation evidence. |

## Revision Criteria

Revise this ADR if:

- the root workspace `rust-version` changes;
- `rust-toolchain.toml` changes the pinned channel or required components;
- Andromeda adopts a newer Rust edition;
- release gates stop claiming Rust 1.95.0 compatibility;
- dependency MSRV enforcement moves from advisory evidence to a different
  blocking tool;
- CI changes the canonical commands for `--locked` release validation;
- a compiler security issue or correctness bug requires emergency toolchain
  governance.

## References

- `AGENTS.md`
- `docs/AGENTS.md`
- `docs/adr/ADR-0005-rust-2024-baseline.md`
- `Cargo.toml`
- `rust-toolchain.toml`
- `Cargo.lock`
- `documentations/governance/msrv-dependency-risk-2026-05-08.md`
- `documentations/governance/supply-chain-policy.md`
- `documentations/governance/release-readiness-gates-2026-05-08.md`
- `documentations/governance/adr-backlog-2026-05-08.md`
