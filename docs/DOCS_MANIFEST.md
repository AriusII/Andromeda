# Documentation manifest

> **Status:** Generated file inventory  
> **Scope:** All files in this `docs/` package

## In this article

- Review the generated file inventory.
- Understand what each folder owns.
- Verify that the package contains only documentation assets.

## Inventory

| Folder | File count | Responsibility |
|---|---:|---|
| Root | 4 | Entry points and style. |
| `project/` | 6 | Doctrine and governance. |
| `architecture/` | 10 | High-level system design. |
| `specifications/` | 22 | Normative technical specifications. |
| `adr/` | 18 | Architecture decisions. |
| `runbooks/` | 10 | Operational incident response. |
| `testing/` | 7 | Quality, test, fuzz, crash, and release gates. |
| `roadmap/` | 13 | Sequenced work with sequence-only language. |
| `operations/` | 5 | Backup, restore, HA/DR, deployment, metrics. |
| `reference/` | 4 | Source basis, terminology, style references. |
| `templates/` | 4 | Authoring skeletons. |

## Generated package constraints

```text
The archive root contains only docs/.
The roadmap contains no calendar commitments.
Markdown files use American English.
Specifications include rejection criteria.
Runbooks include validation and audit evidence.
ADRs define explicit consequences.
```

## Replacement scope

This package is intended to replace the repository `docs/` folder. It does not modify source code, Cargo manifests, CI files, or crate layouts.
