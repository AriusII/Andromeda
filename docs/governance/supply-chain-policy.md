# Supply Chain Policy

## Purpose

This policy defines dependency governance for Andromeda. Machine-readable
enforcement lives in the root `deny.toml`; this document records the human
review expectations around dependency admission, audit evidence, exceptions, and
maintenance.

## Enforcement Configuration

The root `deny.toml` covers:

- advisories: RustSec advisory handling and yanked crate rejection;
- licenses: the allowed license list for third-party crates;
- bans: duplicate-version visibility and wildcard-version rejection;
- sources: allowed registries and git source policy.

## C5 Dependency Rules

Critical C5 crates must:

1. Explicitly list external dependencies.
2. Minimize external dependency count and transitive depth.
3. Prefer `no_std` plus `alloc` where feasible.
4. Justify each external dependency in review.
5. Treat default feature changes as dependency-governance changes.

Acceptable dependency categories include:

- codec and bytes libraries such as `serde`, `prost`, and `bytes`;
- vetted async runtime dependencies such as `tokio`;
- actively maintained TLS and cryptography dependencies such as `rustls` and
  `ring`;
- narrow FFI support such as `libc` when required;
- test-only property-testing dependencies such as `proptest` and `quickcheck`.

Forbidden dependency categories include:

- ad hoc JSON or YAML parsing for release-critical contracts that require typed
  Protobuf or explicit schema ownership;
- embedded dynamic SQL or SRPL engines outside the approved language surfaces;
- FFI to untrusted external code;
- network libraries that have not been reviewed for the RPC/security boundary.

## Audit Process

Before merging dependency changes that affect release-critical paths:

1. Review new direct and transitive dependencies in the pull request.
2. Run advisory and policy checks where available.
3. Inspect duplicate versions and owner paths when the dependency graph changes.
4. Record MSRV, license, advisory, source, duplicate, and feature-surface
   evidence in the owning task or release packet.

Useful local commands:

```powershell
cargo audit
cargo deny check
cargo tree --workspace --locked
cargo tree --duplicates
python tools/testing/supply_chain_preflight.py
```

## Exception Process

To add an exception:

1. File an ADR or governance note with justification, owner, and review date.
2. Add the narrowest possible exception to `deny.toml`.
3. Record the affected dependency paths and release-critical surfaces.
4. Revisit the exception before release and remove it when fixed versions are
   available.

## Maintenance

- Scan for duplicate dependencies during release-readiness review.
- Update `deny.toml` after new security advisories or license-policy changes.
- Prefer targeted lockfile updates, such as `cargo update -p <crate>`.
- Require supply-chain owner review for broad lockfile refreshes.
- Keep `tools/testing/supply_chain_preflight.py` watch edges aligned with this
  policy path.

## References

- `deny.toml`
- `tools/testing/supply_chain_preflight.py`
- Cargo audit: <https://docs.rs/cargo-audit/>
- Cargo deny: <https://embarkstudios.github.io/cargo-deny/>

