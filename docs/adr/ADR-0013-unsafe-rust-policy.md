# ADR-0013: Unsafe Rust Policy

## Status

Accepted

## Purpose

Define the workspace policy for `unsafe` Rust in Andromeda.

Andromeda is a mission-critical relational transactional database engine.
Memory-safety exceptions must stay private, documented, tested, and reviewed.
An `unsafe` block is not an optimization shortcut and is not acceptable as
implicit proof of correctness.

## Decision

Production crates are unsafe-free by default. New production crates should use
`#![forbid(unsafe_code)]` unless a narrower ADR, design record, or crate owner
approval records why an exception is required.

When an exception is approved, the crate or module must use
`#![deny(unsafe_op_in_unsafe_fn)]`, keep the unsafe implementation private,
expose a safe API, and document each unsafe block with a local `SAFETY:`
invariant. The owner must provide tests and review evidence that match the risk
of the code path.

Public `unsafe` APIs are not allowed in production crates unless a later ADR
approves a narrower exception. The default public contract remains safe Rust
types and functions that enforce their own invariants.

## Scope

This ADR applies to:

- production Rust crates in the Andromeda workspace;
- test, fuzz, benchmark, and example code when it is used as release evidence;
- generated Rust code checked into the repository or produced during builds;
- FFI adapters, SIMD intrinsics, OS calls, allocator integrations, memory maps,
  and future GPU adapters;
- documentation or decision records that authorize `unsafe` Rust.

## Non-goals

This ADR does not:

- approve `unsafe` code in any current crate;
- define a SIMD, FFI, memory-mapped I/O, allocator, or GPU implementation;
- weaken the rule that WAL durability precedes visible commit;
- authorize `unsafe` in commit, WAL, rollback, recovery, MVCC visibility,
  catalog publication, or security admission paths;
- replace the need for targeted crash, fuzz, property, Miri, Loom, sanitizer, or
  manual audit evidence when a specific implementation requires it;
- require unsafe-free crates to remove an existing `#![forbid(unsafe_code)]`
  gate.

## Prerequisites

Before approving an unsafe exception, reviewers must confirm:

- the owning crate and subsystem are identified;
- the code path risk is classified as experimental, important, critical, or
  mission-critical;
- no safe Rust alternative satisfies the requirement with acceptable
  performance and complexity;
- the exception does not bypass typed Procedure contracts, WAL durability,
  catalog versioning, IAM, audit, or recovery;
- the validation plan names the tests and review gates required before release.

## Procedure

Use this procedure for any proposed unsafe Rust exception.

1. Keep the unsafe code private.
   - Place unsafe operations behind a safe wrapper that checks the required
     preconditions.
   - Do not expose raw pointers, unchecked lifetimes, unchecked aliasing, or
     public `unsafe` functions as the stable crate contract.
2. Document the invariant locally.
   - Add a `SAFETY:` comment next to each unsafe block.
   - State the required aliasing, lifetime, alignment, initialization,
     thread-safety, and ownership assumptions.
   - Name the safe wrapper or test that enforces the assumption.
3. Restrict critical paths.
   - Do not introduce unsafe code in commit visibility, WAL append or flush,
     rollback, recovery replay, MVCC short-visibility, catalog publication, or
     security-critical admission paths.
   - A later ADR may grant a narrower exception only when it records the exact
     invariant, owner, validation gate, and rollback plan.
4. Classify the validation gate.
   - Important code requires focused unit tests and boundary tests.
   - Critical parser, codec, storage, or protocol code requires property tests
     or fuzz coverage for malformed input.
   - Mission-critical durability, recovery, or concurrency code requires the
     relevant crash/recovery, Miri, Loom, sanitizer, or manual audit evidence.
5. Treat panic and undefined behavior as release blockers.
   - Any `panic`, `unwrap`, or `expect` reachable from unsafe code must be
     removed or justified by an unreachable invariant that is tested.
   - Potential undefined behavior, undocumented unsafe blocks, or unsafe drift
     across an FFI boundary blocks release until reviewed.
6. Keep generated code accountable.
   - Generated unsafe code must be isolated, reproducible, reviewed, and covered
     by the same gates as handwritten unsafe code.
   - If generated unsafe code cannot be audited, the generator output is not
     acceptable for production paths.
7. Preserve review evidence.
   - Link the approving ADR, issue, PR, or review packet from the code comment
     or nearby module documentation.
   - Record the owner responsible for revalidation when the dependency,
     compiler, platform, or hardware target changes.

## Validation

For this documentation-only ADR, validation is textual and structural:

- Confirm that the policy preserves the project invariant that unsafe Rust stays
  private, documented, tested, and reviewed.
- Confirm that the policy does not authorize unsafe code in durability,
  recovery, catalog publication, MVCC visibility, or security-critical paths.
- Confirm that the policy preserves safe public APIs by default.

For future unsafe implementations, reviewers should run the smallest command set
that proves the affected path. Depending on the subsystem, that may include:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
cargo test --doc --workspace
cargo +nightly miri test -p <crate>
cargo fuzz run <target>
```

Durability, recovery, concurrency, and security-critical exceptions also require
targeted crash/recovery, Loom, sanitizer, threat-model, or audit evidence before
release approval.

## Risks

- Performance pressure can turn unsafe Rust into an undocumented shortcut unless
  release gates scan for unsafe drift.
- A safe wrapper can become unsafe in practice if later changes weaken its
  precondition checks.
- Generated or dependency-provided unsafe code can enter the workspace without a
  clear local owner.
- Unsafe code near durability or recovery paths can create corruption modes that
  ordinary unit tests do not expose.

## Troubleshooting

Use this table when reviewing unsafe Rust proposals or drift.

| Symptom | Corrective action |
| --- | --- |
| An unsafe block has no `SAFETY:` invariant. | Reject the change until the invariant, safe wrapper, and validation gate are documented. |
| A public API requires callers to uphold unsafe invariants. | Replace it with a safe API or require a narrower ADR that approves the public unsafe contract. |
| Unsafe code appears in commit, WAL, rollback, recovery, MVCC visibility, catalog publication, or security admission paths. | Treat it as release-blocking drift and escalate to the owning architecture and release reviewers. |
| A panic is reachable from an unsafe block. | Remove the panic path or prove the invariant with tests and review evidence. |
| Generated code contains unsafe operations. | Isolate the generated output, record the generator version, and require the same audit and tests as handwritten unsafe code. |
| A benchmark is the only evidence for an unsafe optimization. | Reject the optimization until correctness, malformed-input, and regression gates pass. |

## References

- `AGENTS.md`
- `docs/AGENTS.md`
- `docs/adr/ADR-0005-rust-2024-baseline.md`
- `docs/adr/ADR-0006-mission-critical-validation.md`
- `documentations/governance/adr-backlog-2026-05-08.md`
- `documentations/governance/decisions/DEC-026-release-gates-and-deferral-policy.md`
