# ADR-0019-POLICY VERSION TERMINOLOGY — PolicyVersion disambiguating terminology

> **Status:** Accepted for V0 documentation baseline
> **Scope:** Andromeda C5 (security/admission/audit) and the procedure-contract,
> catalog, and definition-batch surfaces that bind policy evidence
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, resolver 3, 89 crates

## Context

Andromeda targets an enterprise-grade relational transactional engine with a
strict Procedure surface, typed contracts, WAL-first durability, recovery
evidence, and bounded adaptive internals. Across the V0 normative
specifications, the bare token `PolicyVersion` is used with four
semantically distinct meanings. The cross-spec C5 coherence audit
(`.work/copilot/p01/analysis/c5_coherence_audit.md`, finding **C5-F1, HIGH**)
documents the conflict; it is reproduced here with anchored quotes.

1. **Procedure-contract layer — content digest.**
   `SPEC_PROCEDURE_CONTRACT_V0.md` (§"PolicyVersion form", lines 163–168)
   defines:

   > "`PolicyVersion` is a SHA-256 digest with domain
   > `andromeda.catalog.policy-version.v1.sha256`. It includes
   > `StatsVersion`, `RequiredPermissions`, transaction policy, compatibility
   > policy, result metadata policy, error policy, and multi-result policy."

   In this spec `PolicyVersion` IS a 32-byte SHA-256 digest computed at
   publication time over policy-relevant procedure-contract fields.

2. **Security-admission layer — compound snapshot evidence.**
   `SPEC_SECURITY_ADMISSION_V0.md` (§"PolicyVersion v0", lines 108–122)
   defines `PolicyVersion` as a struct whose fields are:

   > "`policy_version` Monotonic or otherwise totally ordered snapshot
   > identifier within the policy domain. … `policy_digest` Stable digest
   > of the canonical policy snapshot or signed policy bundle. …
   > `policy_domain`, `loaded_at`, `staleness_bound`."

   Here `PolicyVersion` is a compound runtime value whose snapshot
   identifier and digest are independent fields.

3. **Audit-ledger layer — two separate scalar fields.**
   `SPEC_AUDIT_LEDGER_V0.md` (§"Durable audit record format", lines 92–93,
   and §"Data structures", line 42) lists:

   > "`PolicyVersion and PolicyDigest` Required for policy-backed
   > security decisions."

   Audit records record `PolicyVersion` and `PolicyDigest` as two separate
   named scalar fields, mirroring the SECURITY_ADMISSION decomposition.

4. **Definition-batch / catalog layer — scalar snapshot identifier.**
   `SPEC_DEFINITION_BATCH_V0.md` (lines 93–94, 260) requires
   `PolicyVersion` as part of the requesting principal's snapshot evidence
   bound to dry-run, apply preconditions, and audit. `SPEC_CATALOG_OBJECT_MODEL_V0.md`
   (line 76) lists `PolicyVersion` as the "Non-zero policy digest referenced
   by invocation binding" carried in `ProcedureDescriptor` binding evidence.
   At this layer `PolicyVersion` behaves as a scalar identifier of the
   active policy snapshot — the same value the SECURITY_ADMISSION struct
   exposes as its `policy_version` field.

The structural risk is that an implementer can silently pass the
procedure-contract digest into a slot that expects the admission snapshot
identifier (or vice versa) without any normative spec or gate detecting
the substitution. The P01 normative baseline gate
(`tools/testing/p01_spec_baseline_check.py`) validates structural and
token presence, not cross-spec semantic equivalence; finding C5-F1 is
therefore not visible to the gate.

## Decision

Future C5 work — new specifications, new code, new contracts, new
typed errors, new audit fields, and any spec edits that introduce or
rename a policy-evidence token — MUST use the following normative,
disambiguated terminology in place of the bare token `PolicyVersion`:

- **`ContractPolicyDigest`** — the 32-byte SHA-256 digest defined by
  `SPEC_PROCEDURE_CONTRACT_V0.md` §"PolicyVersion form" with hash
  domain `andromeda.catalog.policy-version.v1.sha256`. Computed once at
  procedure-contract publication time. Stable for the lifetime of a
  given `ContractHash`. Carried in `ProcedureContractBinding` and
  `ProcedureDescriptor` binding evidence.

- **`PolicySnapshotId`** — the monotonic (or otherwise totally ordered)
  identifier of the active policy snapshot used at admission time.
  Defined operationally by `SPEC_SECURITY_ADMISSION_V0.md` §"PolicyVersion v0"
  as the `policy_version` field. Consumed by SECURITY_ADMISSION,
  AUDIT_LEDGER, DEFINITION_BATCH, and CATALOG as the runtime binding
  variable. Independent from `ContractPolicyDigest`.

- **`PolicyDigest`** — the 32-byte digest of the canonical policy
  snapshot (or signed policy bundle) loaded at admission time.
  Defined by `SPEC_SECURITY_ADMISSION_V0.md` §"PolicyVersion v0" as
  the `policy_digest` field, and recorded by `SPEC_AUDIT_LEDGER_V0.md`
  as the audit record's `PolicyDigest` field. Independent from
  `ContractPolicyDigest`: it digests the live policy snapshot, not
  contract-relevant fields.

These three names are the canonical vocabulary for cross-spec and
cross-crate communication. The bare token `PolicyVersion` MUST NOT
appear in new specs, new contracts, or new code without a one-line
disambiguating note pointing to one of the three canonical names
above.

## Mapping table

| Existing spec token (location) | Canonical disambiguated name | Semantic |
|---|---|---|
| `PolicyVersion` in `SPEC_PROCEDURE_CONTRACT_V0.md` §"PolicyVersion form" (lines 163–168), §"Canonical field order" (line 122), §"ContractRejectionCode" `POLICY_VERSION_MISSING` (line 200) | `ContractPolicyDigest` | SHA-256 digest computed at publication time over policy-relevant procedure-contract fields. 32-byte content hash. |
| `PolicyVersion` in `SPEC_CATALOG_OBJECT_MODEL_V0.md` §"ProcedureDescriptor binding" (line 76), §"Test plan" (line 292), §"Reject" (line 312) | `ContractPolicyDigest` | Same SHA-256 digest as PROCEDURE_CONTRACT; CATALOG carries it as binding evidence inside `ProcedureDescriptor`. |
| `PolicyVersion` (struct) in `SPEC_SECURITY_ADMISSION_V0.md` §"Data structures" (line 53), §"PolicyVersion v0" (lines 108–122) | Compound: `{ policy_version: PolicySnapshotId, policy_digest: PolicyDigest, policy_domain, loaded_at, staleness_bound }` | Runtime admission evidence: the snapshot identifier (`PolicySnapshotId`) and the loaded snapshot digest (`PolicyDigest`) are independent fields. |
| `policy_version` field in `SPEC_SECURITY_ADMISSION_V0.md` §"PolicyVersion v0" (line 114), `SPEC_AUDIT_LEDGER_V0.md` §"Durable audit record format" (line 93, 152), `SPEC_DEFINITION_BATCH_V0.md` §"Dry-run contract" (line 94), §"Apply preconditions" (line 260, 273) | `PolicySnapshotId` | Monotonic snapshot identifier of the active policy snapshot used at admission time. |
| `policy_digest` field in `SPEC_SECURITY_ADMISSION_V0.md` §"PolicyVersion v0" (line 115), and `PolicyDigest` field in `SPEC_AUDIT_LEDGER_V0.md` §"Durable audit record format" (line 93), §"AdmissionRejectionCode `PolicyDigestMismatch`" (line 240) | `PolicyDigest` | 32-byte digest of the canonical policy snapshot or signed bundle loaded at admission time. |
| `PolicyVersion` in `SPEC_DEFINITION_BATCH_V0.md` §"Required input" (line 94), §"Apply preconditions" (line 260) | `PolicySnapshotId` | Treated as the scalar identifier of the requesting principal's active policy snapshot; aligns with SECURITY_ADMISSION's `policy_version` field. |

## Migration policy

Existing P01 normative specifications are frozen for the P01 baseline
gate. To preserve gate stability, the following rules apply:

1. **P01 specs keep their current token names.** No P01 spec may be
   edited to rename `PolicyVersion`. The
   `tools/testing/p01_spec_baseline_check.py --strict` gate validates
   token presence; renaming would either break the gate or require a
   coordinated baseline update outside the scope of finding C5-F1.

2. **P01 specs MUST add a one-line disambiguating note.** A separate,
   bounded follow-up task (not this ADR) is responsible for adding to
   each affected P01 spec a single normative line of the form
   "This spec's `PolicyVersion` corresponds to ADR-0019
   `<ContractPolicyDigest|PolicySnapshotId|PolicyDigest>`." Such a
   note is purely additive and MUST NOT alter any existing token,
   field name, hash domain, rejection code, audit field, or test plan
   item that the P01 gate inspects.

3. **New specs and new code MUST use the disambiguated names from
   day one.** New normative specifications, new ADRs, new typed
   contracts, new error variants, new audit field names, and any
   newly introduced internal identifier MUST use
   `ContractPolicyDigest`, `PolicySnapshotId`, or `PolicyDigest` and
   MUST NOT introduce a fresh use of the bare token `PolicyVersion`.

4. **Code-level binding rule.** No code path may pass a value typed
   or named as `ContractPolicyDigest` into an API slot that expects
   `PolicySnapshotId` or `PolicyDigest`, or vice versa. Type-level
   distinctions (newtype wrappers in Rust) are the preferred
   enforcement mechanism; a comment-only distinction is insufficient.

5. **Hash domain stability.** This ADR does not change the hash
   domain `andromeda.catalog.policy-version.v1.sha256` defined by
   PROCEDURE_CONTRACT; only the human-facing label changes. The
   on-disk and on-wire byte representations of the digest are
   unchanged.

## Rationale

This decision reduces ambiguity and prevents implementation drift
across architecture, code, tests, and operations. Without
disambiguation, an implementer routing `ContractPolicyDigest` into
the SECURITY_ADMISSION `policy_version` slot would produce silently
wrong admission decisions (the contract digest is not a snapshot
identifier), and an implementer routing `PolicySnapshotId` into the
PROCEDURE_CONTRACT binding evidence slot would defeat policy-drift
detection (the snapshot id is not a content hash of policy-relevant
contract fields).

The three canonical names map one-to-one onto distinct C5
responsibilities:

- `ContractPolicyDigest` — publication-time content evidence.
- `PolicySnapshotId` — admission-time snapshot identity.
- `PolicyDigest` — admission-time snapshot content evidence.

Together they preserve the doctrine that no application execution
proceeds without typed, cataloged, versioned policy evidence, while
removing the cross-spec label ambiguity.

## Consequences

### Positive

- The implementation boundary is explicit: each policy token has
  exactly one semantic meaning across all future specs and code.
- Reviewers can reject incompatible substitutions (digest passed as
  snapshot id, snapshot id passed as digest).
- Tests can be mapped to the decision: type-level newtype tests,
  audit-field mapping tests, and admission-vs-contract binding
  tests all have a single canonical vocabulary to assert against.
- Operational behavior is easier to explain after an incident: a
  policy-binding rejection trace can name the disambiguated token
  it rejected.
- The P01 normative gate is unaffected; the ADR is purely additive.

### Negative

- Some implementation shortcuts are intentionally unavailable: code
  cannot reuse one type for both contract-time and admission-time
  policy evidence.
- Additional tests and documentation are required; in particular,
  the follow-up additive notes in P01 specs and Rust newtype
  wrappers in policy-binding crates.
- Until the additive notes land in P01 specs, readers must consult
  this ADR to disambiguate the bare token `PolicyVersion` in
  existing specs.

## Validation

This ADR is validated by:

- a matching specification when the decision affects a technical
  structure (the additive disambiguating notes in P01 specs are the
  expected matching evidence);
- a test plan when the decision affects runtime behavior
  (newtype-binding tests in security/admission, audit, and
  procedure-contract crates);
- a runbook when the decision affects operations (admission and
  audit rejection traces use the disambiguated names);
- trace or audit evidence when the decision affects security,
  durability, or recovery (audit records distinguish
  `PolicySnapshotId` from `PolicyDigest`, both independent from
  `ContractPolicyDigest`).

## Rejection criteria

Reject implementation work that contradicts this decision without a
superseding ADR. In particular, reject:

- New specs or new code that introduce a fresh use of the bare
  token `PolicyVersion` without an accompanying disambiguating note
  pointing to one of `ContractPolicyDigest`, `PolicySnapshotId`,
  or `PolicyDigest`.
- Code paths that pass `ContractPolicyDigest` into an API expecting
  `PolicySnapshotId` or `PolicyDigest`, or vice versa, including
  via stringly-typed or untyped byte-slice slots.
- Edits to P01 specs that rename existing `PolicyVersion` tokens
  in a way that would alter the
  `tools/testing/p01_spec_baseline_check.py --strict` gate output.
- Audit records that collapse `PolicySnapshotId` and `PolicyDigest`
  into a single field, or that treat `ContractPolicyDigest` as
  interchangeable with either.

## References

- `docs/specifications/SPEC_PROCEDURE_CONTRACT_V0.md` — §"PolicyVersion form" (lines 163–168), §"Canonical field order" (line 122), §"ContractRejectionCode `POLICY_VERSION_MISSING`" (line 200).
- `docs/specifications/SPEC_SECURITY_ADMISSION_V0.md` — §"Data structures" (line 53), §"PolicyVersion v0" (lines 108–122), §"Fail-closed matrix" (lines 238–240, `PolicyVersionMissing`, `PolicyVersionStale`, `PolicyDigestMismatch`).
- `docs/specifications/SPEC_AUDIT_LEDGER_V0.md` — §"Data structures" (line 42), §"Durable audit record format" (lines 92–93, 152), §"Reject" (line 202).
- `docs/specifications/SPEC_DEFINITION_BATCH_V0.md` — §"Required input" (line 94), §"Apply preconditions" (line 260), §"Audit trace fields" (line 273).
- `docs/specifications/SPEC_CATALOG_OBJECT_MODEL_V0.md` — §"ProcedureDescriptor binding" (line 76), §"Reject" (line 312).
- `.work/copilot/p01/analysis/c5_coherence_audit.md` — finding **C5-F1 (HIGH)** and the cross-cluster `PolicyVersion` disambiguation table.
- `docs/project/ANDROMEDA_DOCTRINE.md` — typed cataloged Procedure surface, no commit without policy/admission evidence.
- `docs/adr/ADR-0011-CONTRACT_HASH_CANONICALIZATION.md` — sibling decision on canonical contract hash; the `ContractPolicyDigest` is a peer of `ContractHash` at publication time.
- `docs/adr/ADR-0012-DEFINITION_BATCH_PUBLICATION.md` — sibling decision on DefinitionBatch publication evidence that consumes `PolicySnapshotId` at dry-run and apply.
