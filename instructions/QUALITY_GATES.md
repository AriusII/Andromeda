# Quality Gates

## Purpose

Quality gates make the Andromeda verification matrix executable before code lanes claim readiness. A deliverable is not
complete until the owner records the gate evidence, the command or inspection used, and any residual risk.

## Required gates and execution ownership

- Workspace gate: implementation owner runs the Rust workspace commands and records the exact output summary.
- Doctrine gate: doctrine owner runs terminology and no-go scans for Procedure-only execution, SRPL boundaries, QUIC,
  Protobuf without gRPC, no SQL native surface, no runtime JSON default, and no GPU critical-path drift.
- SRPL gate: SRPL owner verifies bounded language semantics, forbidden constructs, diagnostics, parser/binder/IR
  behavior,
  and stable Procedure contract inputs.
- Catalog gate: catalog owner verifies object identity, dependency ordering, duplicate rejection, contract hashes, and
  Procedure visibility rules.
- Protocol gate: protocol owner verifies Protobuf contract compatibility, QUIC frame mapping, result sequencing,
  completion semantics, and absence of gRPC service surfaces.
- WAL and recovery gate: storage/recovery owner verifies WAL-before-visible-commit, durable LSN evidence, corruption
  boundaries, replay ordering, rollback behavior, and crash reproducibility.
- Storage gate: storage owner verifies page, segment, manifest, checksum/hash, ColdStore immutability, and publication
  invariants.
- Security gate: security owner verifies authorization-before-transaction, least privilege, audit evidence, prompt
  injection considerations where relevant, and no secret exposure.
- Observability gate: observability owner verifies decision traces, correlation fields, durable evidence, protocol
  rejection events, recovery events, and audit completeness.
- Testability gate: test owner verifies unit, integration, property/fuzz, crash/recovery, and reproducibility coverage
  for
  the affected lane.
- Versioning gate: contract owner verifies schema/contract versioning, reserved fields or values, compatibility notes,
  and
  rollback rules.
- Documentation gate: change owner verifies terminology, risks, tests, release blockers, and rollback or disablement
  notes.

## Mandatory workspace commands

Run these commands from the repository root for implementation changes unless the change is explicitly
documentation-only:

```powershell
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

Documentation-only changes must still record that these commands were not run and why. Before a release-readiness claim,
all three commands are mandatory.

## Doctrine scans

Doctrine scans are required before merging any change that affects contracts, workflow, storage, protocol, Procedure
execution, observability, or security. Use the existing hook patterns when available and supplement with repository
scans.
The reviewer must classify each match as an approved doctrine reference, a test fixture, or a violation.

Minimum scans:

```powershell
rg -n -i "\bgrpc\b" .
rg -n -i "\bad\s*hoc\s+sql\b|\bsql\s+native\s+surface\b|\bselect\s+\*\b" .
rg -n -i "\bruntime\s+json\b|\bjson\s+default\b|\bdefault\s+wire\s+format\b" .
rg -n -i "\bgpu\b.*\b(commit|wal|rollback|recovery|critical path)\b|\b(commit|wal|rollback|recovery|critical path)\b.*\bgpu\b" .
rg -n "\bunsafe\b|#!\[.*forbid\(unsafe_code\).*\]" crates instructions workflows registries hooks
```

Expected doctrine result:

- Procedure remains the application execution unit.
- SRPL remains the bounded Procedure language; it is not renamed SQL.
- QUIC remains the transport.
- Protobuf remains the contract serialization format without gRPC.
- No SQL native surface is introduced.
- Runtime JSON is not introduced as the default wire format.
- GPU work is not allowed in commit, WAL, rollback, recovery, or other critical durability paths.
- Unsafe Rust is absent or, if ever introduced by decision record, isolated behind a reviewed safe API with documented
  invariants.

## Verification matrix

| Area          | Owner                  | Required verification                                                                                                                                                               | Evidence                                                                                   |
|---------------|------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------|
| Workspace     | Implementation owner   | `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo test --workspace`.                                                                                                  | Command names, pass/fail status, and relevant failure summary.                             |
| Doctrine      | Doctrine owner         | Scans for gRPC, runtime JSON default, SQL native surface, `SELECT *`, unsafe drift, and GPU critical-path drift.                                                                    | Scan command, classification of matches, and violation disposition.                        |
| SRPL          | SRPL owner             | Lexer/parser tests, forbidden construct tests, binder diagnostics, IR golden tests, bounded cardinality/resource behavior.                                                          | Test names plus examples of rejected constructs and diagnostics.                           |
| Catalog       | Catalog owner          | Duplicate-name tests, dependency-order tests, `DefinitionBatch` dry-run/apply tests, Procedure contract identity/hash/version checks.                                               | Contract hash/version evidence and rejection trace evidence.                               |
| Protocol      | Protocol owner         | Payload/frame code lockstep tests, metadata-before-payload tests, completion/error conversion tests, Protobuf compatibility checks, no gRPC service definitions.                    | Compatibility notes, numeric mapping checks, and protocol rejection traces.                |
| WAL/recovery  | Storage/recovery owner | Record codec tests, gapless LSN tests, checksum/truncation tests, flush-boundary tests, crash scenarios, committed-only replay, incomplete transaction handling, manifest fallback. | Durable LSN evidence, corruption boundary evidence, and reproducible crash seed or script. |
| Storage       | Storage owner          | Page/trailer validation, segment publication, ColdStore immutability, checksum/hash tests, manifest switch validation.                                                              | Hash/checksum evidence, manifest validation result, and publication trace.                 |
| Security      | Security owner         | Permission matrix, authorization rejection before transaction creation, audit presence, least privilege, no secret leakage.                                                         | Denial trace, audit event, and reviewed secret-scan disposition.                           |
| Observability | Observability owner    | Trace correlation, durable LSN evidence, recovery/manifest/protocol rejection events, audit completeness, event ID stability.                                                       | Trace IDs, correlation fields, event schema/version evidence, and omission review.         |

## Release blockers

Any of the following blocks release or readiness approval until fixed or explicitly deferred by decision record:

- Recovery failure, including replay divergence, invalid manifest fallback, or inability to stop at the last valid WAL
  record.
- Visibility violation, including commit visibility before durable WAL evidence or authorization/contract validation
  after
  transaction creation.
- Contract mismatch acceptance across Procedure ID, contract hash, catalog version, Protobuf contract, or QUIC frame
  mapping.
- Audit omission for contract rejection, authorization rejection, WAL append/flush, commit visibility, rollback,
  recovery,
  catalog mutation, protocol rejection, or corruption boundary.
- Panic in a critical path, including admission, authorization, commit, WAL, rollback, recovery, storage publication,
  security, or protocol validation.
- Silent corruption, including checksum/hash mismatch acceptance, WAL gap acceptance, replay past corruption, or result
  stream sequence corruption.
- Non-reproducible crash test, including crash/recovery failures without deterministic seed, scenario, durable boundary,
  or
  replay instructions.

## Pass criteria

A deliverable passes only when it states what changed, why it is safe, how it is verified, and how it can be rolled back
or disabled.

For vertical-slice work, the pass statement must also identify the affected matrix areas, the gate owners, and whether
any
release blocker was observed.
