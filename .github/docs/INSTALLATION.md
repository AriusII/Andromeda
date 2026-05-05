# Installation

Extract this archive at the root of the Andromeda repository.

```bash
unzip Andromeda_GitHub_Actions_Pack_2026.zip -d .
git add .github deny.toml .markdownlint-cli2.jsonc SECURITY.md CONTRIBUTING.md
git commit -m "ci: add Andromeda GitHub Actions operating pack"
git push origin main
```

For true pre-merge enforcement, use a short-lived branch and open a pull request into `main`.
