# Andromeda Doctrine for Codex

## Purpose

Provide the minimum doctrine Codex must preserve.

## Core doctrine

Andromeda is strict at the boundaries and adaptive inside.

Strict boundaries:

- Type System
- Procedure Contract
- Catalog
- Transaction
- WAL
- Network
- Security
- Import
- Audit
- Recovery

Adaptive internals:

- Optimizer
- Plan cache
- Statistics
- Hardware
- Maps
- Backpressure

## C5 examples

- Durable WAL before visible commit.
- System Database correctness.
- Recovery after crash.
- Security audit for critical operations.
- Catalog versioning for changes.

## Rejection rule

Reject a design if it is not definable, typed, bounded, observable, versioned, recoverable, explainable, and disableable.
