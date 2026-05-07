# AI Operating Architecture for Andromeda

## Overview

The AI operating layer is a controlled assistant substrate for the Andromeda project. It is not part of the database
engine runtime. It supports design, specification, implementation review, documentation, research synthesis, and
validation.

The architecture uses five asset classes:

| Asset           | Responsibility                                  |            Loaded by default |       May call tools | May orchestrate |
|-----------------|-------------------------------------------------|-----------------------------:|---------------------:|----------------:|
| Instruction     | Stable policy or standard                       |           Yes, when selected |                   No |              No |
| Prompt template | User-invoked task shape                         |                           No |                   No |              No |
| Agent           | Specialized worker with scope and handoff rules |                           No |      Yes, if enabled |             Yes |
| Skill           | Focused capability with repeatable procedure    | Only metadata until relevant | Optional, restricted |              No |
| Hook            | Lifecycle control point                         |                          N/A |  Executes externally |        Enforces |

## Core design rules

1. Instructions define behavior and constraints.
2. Agents own a domain and compose skills.
3. Skills solve one narrow capability.
4. Hooks enforce boundaries before and after actions.
5. Registries make relationships auditable.
6. Prompt templates standardize task requests but do not override project invariants.

## Andromeda-specific constraints

- No generated content may normalize ad hoc SQL as a native surface.
- No generated contract may introduce gRPC.
- Protobuf is allowed only as a contract and serialization layer over QUIC.
- Runtime JSON is not a default protocol surface.
- Any change to SRPL, contracts, catalog, WAL, storage formats, or security policy must produce a decision record.
- Any optimization proposal must include observability, fallback, and disablement behavior.
- Any unsafe Rust design must include an invariant comment, safe wrapper plan, fuzz/property testing, and review gate.

## Layered flow

```text
User task
  -> Prompt template or direct request
  -> Instruction set
  -> Lead agent selection
  -> Skill activation as needed
  -> Tool calls guarded by hooks
  -> Output validation
  -> Decision record or artifact
  -> Registry update when durable
```

## Agent selection

Use a single lead agent for each task. The lead agent may hand off to another agent when the task crosses a domain
boundary. Handoffs must include:

- Original task.
- Current assumptions.
- Evidence collected.
- Files touched.
- Decisions already made.
- Open questions.
- Expected output.

## Skill activation

A skill should activate only when its description matches the task. A skill must not become a general-purpose agent. If
a skill needs more than one page of instructions, place details in supporting files and keep `SKILL.md` concise.

## Hook enforcement

Hooks are the last non-human guardrail. They should prevent clearly dangerous or project-violating operations, warn
about suspicious changes, and require completion checks before stopping.

Use hooks for policy enforcement, not for complex architecture reasoning.
