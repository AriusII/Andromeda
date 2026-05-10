# Andromeda documentation

> **Status:** Replacement documentation package  
> **Scope:** `docs/` only  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition

## In this article

- Understand the purpose of this documentation package.
- Navigate architecture, specifications, ADRs, runbooks, testing, and roadmap files.
- Apply the replacement instructions safely.
- Preserve Andromeda's strict, procedure-only, WAL-first doctrine.

## Purpose

This `docs/` folder is a clean replacement documentation set for Andromeda.
It is designed for a modern relational transactional database engine that exposes a strict native surface:

```text
QUIC + custom typed RPC + cataloged Procedure + SRPL + typed ResultStream
```

Andromeda is not documented here as a generic SQL server. The application surface is procedure-only, contract-first, typed, observable, and transaction-scoped.

## Read first

| File | Purpose |
|---|---|
| `INDEX.md` | Main reading map and documentation structure. |
| `DOCS_MANIFEST.md` | Inventory of all generated files and their role. |
| `project/ANDROMEDA_DOCTRINE.md` | Non-negotiable doctrine and invariants. |
| `architecture/ENGINE_OVERVIEW.md` | High-level architecture and engine boundaries. |
| `roadmap/ROADMAP.md` | Sequenced roadmap without calendar promises. |
| `specifications/` | Normative technical specifications. |
| `adr/` | Architecture Decision Records. |
| `runbooks/` | Operational response procedures. |
| `testing/` | Test strategy, crash matrix, fuzzing, and release gates. |

## Replacement instruction

Delete your local `docs/` folder, then copy this new `docs/` folder at the repository root.

```text
repo-root/
  docs/              <- replace with this folder
  crates/
  Cargo.toml
```

Do not merge old files manually unless a specific old file contains unique implementation evidence that is not represented in this package.

## Documentation principles

- Use American English.
- Prefer short, precise sentences.
- Use stable headings.
- Avoid hidden assumptions.
- Do not include calendar promises in roadmap files.
- Describe sequencing with entry criteria, exit criteria, dependencies, and acceptance checks.
- Keep each document responsible for one topic.

## Core invariant summary

```text
No ad hoc SQL application surface.
Every application execution goes through a cataloged Procedure.
Every Procedure has a typed, hashed, versioned contract.
Every Procedure is transaction-scoped.
No visible commit without durable WAL.
RAM is not system truth.
GPU is never part of commit, rollback, WAL, recovery, MVCC visibility, or security-critical authorization.
Every critical decision must be observable and explainable.
```
