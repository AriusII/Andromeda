# Governance

This directory is the `/docs` home for repository governance. These pages record
release expectations, risk disposition, and supply-chain policy; they do not
approve a release by themselves.

## Documents

- `release-gates.md` defines the release gates and retained evidence expected
  before C4/C5 readiness claims.
- `risk-register.md` lists current release-governance risks and required
  disposition before promotion.
- `supply-chain-policy.md` records dependency admission, audit, exception, and
  maintenance policy for `deny.toml` and supply-chain validation tooling.

## Rules

- Release evidence must name the exact command, source state, toolchain,
  result, artifact path, residual risk, and reviewer.
- Documentation, benchmark output, RAM state, temporary files, and advisory
  smoke checks are not durable truth.
- Supply-chain changes remain governed by `supply-chain-policy.md`.
