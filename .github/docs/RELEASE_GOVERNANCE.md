# Release Governance

Use semantic version tags such as `v0.1.0`.

Release targets:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`

Release requirements:

- Build with `--release`.
- Use `Cargo.lock`.
- Generate SHA-256 checksums.
- Generate artifact provenance attestations.
- Publish through GitHub Releases.

Future hardening:

- Replace action tags with full commit SHAs.
- Add Sigstore signing policy if required.
- Add independent reproducible build verification.
