---
applyTo: ".github/workflows/**/*.yml"
---
# GitHub Actions Instructions
- Use minimal `permissions` blocks.
- Use `concurrency` for every workflow.
- Prefer read-only permissions unless a job publishes, comments, attests, or releases.
- Keep fast required checks separate from deep scheduled checks.
- Do not introduce secret-dependent logic in pull request workflows from forks.
- For maximum hardening, replace tags with full-length commit SHAs after initial validation.
