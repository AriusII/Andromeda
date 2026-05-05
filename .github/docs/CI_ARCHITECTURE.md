# Andromeda CI Architecture

This pack separates fast gates from deep validation.

| Workflow | Purpose |
|---|---|
| `00-ci.yml` | Required fast quality gate. |
| `01-rust-matrix.yml` | Rust matrix validation. |
| `02-protobuf-contracts.yml` | Protobuf contract governance and gRPC exclusion. |
| `03-security-codeql.yml` | CodeQL and workflow security analysis. |
| `04-dependency-review.yml` | Pull request dependency review. |
| `05-supply-chain.yml` | cargo-audit and cargo-deny. |
| `06-nightly-deep-validation.yml` | Miri, sanitizers, and unused dependency checks. |
| `07-fuzzing.yml` | Scheduled fuzzing smoke tests. |
| `08-performance.yml` | Criterion benchmark smoke and artifact capture. |
| `09-cross-build.yml` | Linux x64 and ARM64 cross-build gates. |
| `10-release.yml` | Tag-driven release packaging and artifact attestations. |
| `11-docs-links.yml` | Markdown linting and link checks. |
| `12-github-hygiene.yml` | actionlint and YAML checks. |
| `13-copilot-governance.yml` | AI instruction validation. |
| `14-housekeeping.yml` | Scheduled cleanup of old workflow runs. |
| `15-crash-recovery-placeholder.yml` | Placeholder gate for WAL/recovery tests. |
