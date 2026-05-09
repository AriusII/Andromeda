# MSRV Policy

**Generated**: 2026-05-08  
**Current MSRV**: Rust 1.85 (Stable 2024)  
**Policy Effective**: Phase 0 → Ongoing  
**Purpose**: Stable, predictable Rust version baseline

## Policy Statement

**Andromeda MSRV = Stable Rust release from start of calendar year.**

- **2024**: Stable 2024 = Rust 1.85 ✅
- **2025**: Stable 2025 = Rust 1.?? (TBD end-2024)
- **2026**: Stable 2026 = Rust 1.?? (TBD end-2025)

---

## Why Not Nightly?

### ❌ Nightly is Not Suitable

1. **Breakage Risk**: Nightly features change weekly → reproducibility nightmare
2. **Release Stability**: Production database can't ride nightly unstable features
3. **Dependency Hell**: Any nightly-dependent crate blocks upgrades
4. **CI/CD Friction**: Nightly build failures at unpredictable times
5. **Support Burden**: "It worked on my nightly" is not enterprise-grade

### ✅ Stable is Suitable

1. **Predictable Breakage**: ~6-week release cycle is known
2. **LTS-Like**: Stable 2024 remains stable, won't regress
3. **Enterprise Grade**: Production systems can plan upgrades
4. **Dependency Hygiene**: Clear paths through ecosystem
5. **Support Model**: "Here's the Rust version we tested on"

---

## Why Not Older Stable?

**MSRV = Stable 2024** ensures:

1. **Modern Tooling**: Stable 2024 includes essential ecosystem tools
   - `cargo nextest` tested on 1.85+
   - `cargo clippy` lint levels stable
   - `cargo deny` works reliably

2. **Standard Features**: Stable 2024 is modern enough for:
   - Generics constraints (const generics tested)
   - Async/await (mature)
   - Zero-copy serialization patterns (needed for WAL)
   - Error handling traits (stable)

3. **Dependency Baseline**: Most production crates require 1.70+
   - `tokio` (async runtime)
   - `quinn` (QUIC)
   - `serde` (serialization)

---

## MSRV Changes

### Rule 1: MSRV Bumps Require ADR

Any proposal to bump MSRV must:

1. Create ADR with title: `ADR-XXX: Bump MSRV to Rust 1.YY`
2. Justify with specific features needed
3. Analyze impact on CI, dependencies, external consumers
4. Link to usage evidence (crate requirements)
5. Get approval from architecture team

### Rule 2: MSRV Bumps Are Backward-Incompatible

MSRV bumps are **semver-major** changes:

```toml
# Before bump:
rust-version = "1.85"  # Allows 1.85, 1.86, 1.87, ...

# After bump:
rust-version = "1.90"  # Only allows 1.90+
```

This forces:
- Major version bump (0.major+1.0)
- Release notes: "Requires Rust 1.90+"
- Consumer notification

### Rule 3: One Bump Per Release

Don't bump MSRV multiple times in single release. Batch with quarterly reviews.

---

## Current Policy (Phase 0)

| Item | Status | Details |
|------|--------|---------|
| **MSRV** | ✅ Locked | Rust 1.85 (Stable 2024) |
| **Bumps** | 🔒 Forbidden | Unless ADR approved |
| **Nightly** | ❌ Forbidden | No nightly code in mainline |
| **Beta** | ❌ Forbidden | Stable releases only |
| **EOL Plan** | 📊 TBD | 1.85 support until end-2025 |

---

## Version Baseline

### Rust Toolchain

```toml
# In rust-toolchain.toml (required)
[toolchain]
channel = "1.85"
components = ["rustfmt", "clippy", "rust-analyzer"]
targets = ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc", "aarch64-unknown-linux-gnu"]
```

### Cargo Workspace

```toml
# In Cargo.toml (root)
[workspace]
members = ["crates/*"]

# All crates must specify:
# [package]
# rust-version = "1.85"
```

---

## Verification

### CI Requirements

```yaml
# .github/workflows/msrv-check.yml
name: MSRV Check
on: [push, pull_request]

jobs:
  msrv:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        rust: ["1.85", "stable"]  # Test both MSRV and latest stable
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ matrix.rust }}
      - run: cargo check --workspace
      - run: cargo test --workspace
```

### Local Verification

```bash
# Check against MSRV before PR
rustup install 1.85
rustup override set 1.85
cargo check --workspace
cargo clippy --workspace
cargo test --workspace

# Check against stable too
rustup override unset
cargo check --workspace
```

---

## Upgrade Path (Future)

### Timeline

| Year | Event | Action |
|------|-------|--------|
| Q4 2024 | Rust 2025 released | Plan for Phase 8 |
| Q1 2025 | Create ADR for bump to Rust 1.?? | Review + approve |
| Q2 2025 | Release Andromeda 1.0.0 | Update MSRV |
| Q3 2025 | Stable 1.85 reaches EOL | Drop support notice |

### Bump Proposal Template

When MSRV bump becomes necessary:

```markdown
# ADR-XXX: Bump MSRV to Rust 1.YY

## Context
- Current MSRV: 1.85 (stable 2024)
- Need for bump: Stabilization of [feature]
- Feature used in: [C5 crate name]
- First available: Rust 1.YY (released DATE)

## Decision
Bump to Rust 1.YY in release X.0.0 (semver-major bump)

## Impact Analysis
- Dependencies affected: [list]
- CI/CD changes: [describe]
- External consumer notice: [describe]
- Support window: 1.YY through end-DATE

## Alternatives Considered
1. Polyfill for 1.85 (cost vs benefit)
2. Delay until 1.85 EOL (timeline)
3. Conditional compilation (complexity)
```

---

## FAQ

### Q: Can we use nightly features in feature-gated code?

**A**: ❌ No. Even gated nightly code:
- Makes CI unstable (nightly changes weekly)
- Complicates testing (need nightly CI lane)
- Violates enterprise stability goal

If feature is needed, wait for stabilization or polyfill for MSRV.

---

### Q: What if dependency requires newer Rust?

**A**: Analyze dependency update:

```bash
# Check what version is needed
cargo update --package tokio --dry-run

# If upgrade requires Rust 1.90:
# Option 1: Bump MSRV (requires ADR)
# Option 2: Stay on older version of dependency
# Option 3: Create polyfill/wrapper to avoid update
```

Choose option based on:
1. Is new dependency essential? (C5 path vs C0 path)
2. How much work is polyfill? (ROI)
3. How long until dependency supports 1.85? (timeline)

---

### Q: Do we test on multiple Rust versions?

**A**: ✅ Yes:
- CI tests on MSRV (1.85) + latest stable
- Catches MSRV regressions early
- Ensures forward compatibility

---

### Q: What about ARM64 targets?

**A**: Supported on MSRV 1.85:
```bash
rustup target add aarch64-unknown-linux-gnu
cargo check --target aarch64-unknown-linux-gnu
```

---

## Governance

### MSRV Decisions

| Decision | Authority | Process |
|----------|-----------|---------|
| Yearly review | Architecture team | Scheduled Q4 |
| Mid-year bump proposal | Crate owner + 2 reviewers | ADR + approval |
| Emergency bump | Release team | Documented with justification |
| Drop support | Maintenance team | Communicated 6 months prior |

### Communication

- **MSRV changes**: Announce in release notes, `rust-version` in `Cargo.toml`
- **EOL dates**: Document in README.md when Rust version reaches EOL
- **Upgrade guidance**: Provide migration guide for consumers

---

## Tools

### Check MSRV Compliance

```bash
# Verify all crates have rust-version
grep -r "rust-version" crates/*/Cargo.toml | wc -l
# Should equal number of crates

# Verify versions match
grep -h "rust-version" crates/*/Cargo.toml | sort -u
# Should all be "1.85"

# Test on MSRV
cargo +1.85 check --workspace
cargo +1.85 test --workspace
```

### Cargo Feature Validation

```bash
# Check what MSRV blocking features exist
cargo metadata --format-version 1 \
  | grep -E "rust_version|features" | head -20
```

---

## Next Steps

1. **Phase 0**: Lock MSRV policy ✅
2. **Phase 1-7**: Maintain 1.85 compliance
3. **Q4 2024**: Plan for Rust 2025 bump (ADR)
4. **Q1 2025**: Review ADR, decide on bump
5. **Q2 2025**: Release with updated MSRV

## References

- Rust Releases: https://releases.rs/
- Cargo Manifest: https://doc.rust-lang.org/cargo/reference/manifest.html
- rustup: https://rustup.rs/
