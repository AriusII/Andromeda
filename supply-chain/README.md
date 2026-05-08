# Supply Chain Governance

## Purpose

This directory contains supply chain security and dependency governance policies for Andromeda.

## Contents

### deny.toml

Root deny configuration (../deny.toml) specifies:
- **Advisories**: Security vulnerability tracking (yanked crate rejection)
- **Licenses**: Permitted license list (Apache-2.0, BSD-2/3-Clause, MIT, ISC, Zlib, Unicode)
- **Bans**: Multiple versions detection, wildcard version restrictions
- **Sources**: Registry and git source whitelist

### C5 Dependency Rules

**Critical (C5) crates** must:
1. Explicitly list all external dependencies
2. Minimize external dependencies
3. Prefer no_std + alloc where feasible
4. Avoid deep transitive chains
5. Include justification for each external dependency

**Categories of acceptable dependencies**:
- Codec: serde, prost, bytes (well-maintained, well-known)
- Async: tokio (vetted, industry standard)
- TLS: rustls, ring (cryptographic, actively maintained)
- Core: libc (for FFI only, when necessary)
- Testing: proptest, quickcheck (property testing)

**Forbidden categories**:
- Ad hoc JSON/YAML parsing (use typed Protobuf)
- Embedding dynamic SQL/SRPL engines
- FFI to untrusted external code
- Network libraries not vetted for RPC

### Audit Process

1. Run `cargo audit` before merging to main
2. Run `cargo deny check` as part of CI
3. Review new dependencies in pull request comments
4. Document exception path if override needed

### Exception Process

To add an exception:
1. File an ADR with justification
2. Add entry to deny.toml with `ignore = [advisory_id]`
3. Document the exception and review period
4. Set reminder to re-evaluate before release

### Maintenance

- Scan for duplicate dependencies monthly
- Update deny.toml after new security advisories
- Remove exceptions when fixed versions are available
- Audit transitive dependency changes in lock file

## Commands

```bash
# Check supply chain
cargo audit
cargo deny check

# View dependency tree
cargo tree
cargo tree -p <crate-name>

# Find duplicate versions
cargo tree --duplicates

# Update lock file safely
cargo update --aggressive  # In separate PR with review
```

## References

- Root deny.toml (../deny.toml)
- Cargo audit: https://docs.rs/cargo-audit/
- Cargo-deny: https://embarkstudios.github.io/cargo-deny/
