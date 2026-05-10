# Markdown style guide

> **Status:** Documentation authoring standard  
> **Audience:** Documentation authors and reviewers

## In this article

- Write Markdown in a consistent technical style.
- Use headings, tables, code blocks, and admonitions predictably.
- Avoid roadmap ambiguity.
- Keep documents friendly to humans, CI, and AI-assisted review.

## Writing style

Use direct American English.

Prefer:

```text
The WAL record must include a CRC64.
```

Avoid:

```text
It would probably be good if the WAL record maybe contained some checksum.
```

## Heading rules

- Use exactly one H1 per file.
- Use sentence-style capitalization for headings.
- Use stable names for sections that CI can search.
- Do not use decorative headings.

## Required top section

Every durable documentation file should start with:

```markdown
# Title

> **Status:** ...
> **Audience:** ...
> **Baseline:** Rust 1.95.0

## In this article
```

## Tables

Use tables for stable matrices:

- invariants;
- responsibilities;
- allowed and forbidden actions;
- error families;
- acceptance checks;
- risk registers.

## Code blocks

Use fenced code blocks with a language identifier when possible.

Use `text` for abstract state machines and pipelines.

```text
Invocation
-> Admission
-> TransactionScope
-> WAL
-> Commit
```

## Admonitions

Use Microsoft-style Markdown admonitions sparingly.

```markdown
> [!IMPORTANT]
> No visible commit is valid before durable WAL.
```

Allowed kinds:

| Kind | Use |
|---|---|
| `[!IMPORTANT]` | Non-negotiable rule. |
| `[!CAUTION]` | Risky behavior or dangerous shortcut. |
| `[!NOTE]` | Clarification. |
| `[!TIP]` | Practical guidance. |

## Roadmap writing rules

Roadmap files must not contain calendar commitments, sprint promises, calendar quarters, or completion estimates.

Use:

```text
Entry criteria
Exit criteria
Dependencies
Acceptance checks
```

Do not use:

```text
Calendar commitment
Completion promise
Schedule estimate
```

## Precision rules

Every specification must include:

```text
Purpose
Scope
Non-goals
Data structures
Invariants
Serialization
State transitions
Error model
Security model
Observability
Recovery behavior
Compatibility
Tests
Rejection criteria
```

## Link policy

Prefer relative links inside `docs/`.

Do not link to local machine paths, temporary files, or sandbox paths inside repository documentation.
