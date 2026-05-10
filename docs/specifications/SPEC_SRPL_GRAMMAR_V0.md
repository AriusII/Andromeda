# Specification: SRPL Grammar v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the purpose and scope of `SRPL Grammar v0`.
- State the required structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Define the minimal strict SRPL grammar for transaction-scoped Procedures. V0 is a
closed grammar: every accepted token and production must map to a typed AST node,
and every rejected surface must produce a stable diagnostic code.

## Scope

This specification applies to V0 documentation and implementation planning. It
defines the minimum stable contract needed for parser code, parser diagnostics,
DefinitionBatch dry-runs, contract lowering, tests, and review.

V0 accepts one procedure declaration per source unit. The parser may expose both
the current narrow `body { ... }` form and the migration `begin ... end` form,
but both forms are bounded and both lower to the same ordered operation list.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only surface.
- It does not authorize hidden dynamic behavior.
- It does not authorize any ad hoc SQL, SQL fragment, or SQL-like expression
  language inside SRPL.
- It does not introduce loop execution. `for_each` is reserved for a later
  bounded-loop specification.
- It does not introduce ambient nullability. Optionality is represented only by
  explicit cardinality or a later explicit branch construct.

## Data structures

| Structure | Required role |
|---|---|
| `procedure_decl` | Single top-level declaration with qualified procedure name, ordered inputs, one declared result stream, and optional bounded body. |
| `parameter_decl` | Ordered named input field with a required scalar type. Ambient absence syntax is rejected. |
| `result_decl` | Exactly one named result stream with explicit `Cardinality` and at least one named required column. |
| `body_decl` | Ordered list of at most 16 operations. The limit is part of `bounded_loop_policy`, not an implementation detail. |
| `ensure_stmt` | Begin/end guard over one resolved source, one binding, equality predicate, quantity predicate, and explicit failure code. |
| `read_stmt` | Bounded read statement over catalog-resolved objects with explicit binding and explicit cardinality. |
| `assert_stmt` | Typed assertion that can reject before mutation using an explicit failure code. |
| `update_stmt` | Mutation statement requiring later binder proof of write permission and WAL-covered target. |
| `emit_stmt` | ResultStream emission statement bound to the declared output stream and explicit value list. |
| `raise_stmt` | Typed rejection statement with an explicit diagnostic or business failure code. |
| `return_stmt` | Begin/end emission form equivalent to `emit_stmt`; it must bind to the declared output stream. |
| `let_stmt` | Reserved in V0 unless later specified with exact type, absence, cardinality, and effect rules. |
| `for_each_stmt` | Reserved in V0; no positive loop construct is active until a bounded loop spec exists. |
| `diagnostic_code` | Stable parser diagnostic code with source span, phase, severity, and non-contractual message. |
| `bounded_loop_policy` | V0 policy is "no loops"; body operation count is statically bounded at 16 and loop keywords are rejected. |

## Concrete grammar

The grammar below is normative for V0. Whitespace and comments are not semantic.
Formatting-only changes must not affect the AST, ProcedureContract candidate, or
diagnostic codes.

```text
source          ::= procedure_decl EOF
procedure_decl  ::= "procedure" qualified_name "accepts" field_list
                    "returns" result_decl procedure_tail
procedure_tail  ::= ";"
                  | body_decl [";"]
                  | begin_end_decl [";"]
                  | empty

field_list      ::= "(" [field_decl {"," field_decl}] ")"
field_decl      ::= identifier scalar_type
result_decl     ::= identifier cardinality nonempty_field_list
nonempty_field_list ::= "(" field_decl {"," field_decl} ")"

body_decl       ::= "body" "{" {body_operation ";"} "}"
body_operation  ::= read_stmt | assert_stmt | update_stmt | emit_stmt | raise_stmt
read_stmt       ::= "read" qualified_name identifier cardinality
assert_stmt     ::= "assert" identifier identifier
update_stmt     ::= "update" qualified_name identifier
emit_stmt       ::= "emit" identifier identifier_list
raise_stmt      ::= "raise" identifier

begin_end_decl  ::= "begin" {begin_end_operation [";"]} "end"
begin_end_operation ::= ensure_stmt | update_set_stmt | return_stmt
ensure_stmt     ::= "ensure" qualified_name identifier "where" identifier "="
                    scoped_field "and" scoped_field ">=" identifier
                    "else" "fail" identifier
update_set_stmt ::= "update" qualified_name "set" identifier "=" scoped_field
                    "-" identifier "where" identifier "=" scoped_field
                    "affected" "rows" number
return_stmt     ::= "return" identifier identifier_list

identifier_list ::= "(" identifier {"," identifier} ")"
qualified_name  ::= identifier {"." identifier}
scoped_field    ::= identifier "." identifier
```

## Type and absence syntax

SRPL V0 field types are required scalar values. The accepted scalar surface is:

```text
scalar_type ::= integer_type
              | "bool"
              | "text" ["(" number ")"]
              | "decimal" "(" number "," number ")"
              | "timestamp" "(" timestamp_mode ")"
integer_type ::= "i8" | "i16" | "i32" | "i64" | "i128"
               | "u8" | "u16" | "u32" | "u64" | "u128"
timestamp_mode ::= "transaction" | "invocation" | "monotonic_epoch"
```

The parser must reject `null`, `nullable`, `optional`, `option`, and `maybe` as
types, field names, stream names, binding names, scoped-field parts, and emitted
value names. The diagnostic must explain that SRPL has no ambient NULL semantics
and that absence is represented by explicit cardinality or later branch syntax.

`optional one` and `optional_one` are cardinality syntax, not nullable column
syntax. A result stream with `optional one` may emit zero or one row; every
emitted column still has a required type unless a later type-system revision
adds an explicit optional field descriptor.

## Cardinality syntax

The only accepted cardinality forms are:

| Source form | Canonical cardinality | Minimum rows | Maximum rows |
|---|---|---:|---:|
| `one` | `One` | 1 | 1 |
| `optional one` | `OptionalOne` | 0 | 1 |
| `optional_one` | `OptionalOne` | 0 | 1 |
| `many` | `Many` | 0 | Binder/resource policy bound |
| `nonempty many` | `NonEmptyMany` | 1 | Binder/resource policy bound |
| `non_empty_many` | `NonEmptyMany` | 1 | Binder/resource policy bound |

The parser must reject `optional many`, `optional_many`, `nonempty one`, and
`non_empty_one` with stable diagnostics. `many` and `nonempty many` are accepted
only as cardinality declarations; later binder and resource policy phases must
prove read/write set bounds before execution planning.

## No SQL surface

No dynamic SQL is allowed in SRPL V0. The grammar has no SQL terminal, no string
literal execution terminal, no raw predicate text terminal, no table-name string
terminal, and no escape hatch that can carry executable text.

The parser or pre-parser forbidden-construct scanner must reject:

- `select *`, `select`, `insert`, `delete`, `merge`, or SQL-like clauses used as
  executable text.
- Reject `dynamic SQL`, `execute SQL`, `exec`, or any construct that describes
  building executable database text at runtime.
- Quoted or unquoted table names supplied as dynamic values instead of
  `qualified_name`.
- Raw expression strings standing in for `predicate`, `mutation`, `where`, or
  `return` semantics.

This scanner is a diagnostic defense. The authoritative invariant is the typed
grammar: accepted AST nodes must use qualified names, identifiers, numeric
literals, enum operation kinds, and explicit scalar descriptors, never executable
source text.

## Loop and bound policy

V0 has no positive loop construct. `while`, `loop`, recursive procedure calls,
and `for_each` are rejected. The token `for_each` remains reserved for a later
specification that must include:

- A statically known source relation.
- A maximum iteration count or catalog/resource-policy proof.
- An explicit read/write set contribution.
- Cardinality preservation rules.
- Stable diagnostics for unbounded or side-effecting loop bodies.

Until that later specification exists, `reserved for_each` is a required parser
rejection and body length is the only V0 iteration-like bound. A procedure body
must contain no more than 16 operations after parsing either body form.

## Diagnostic codes

Every parser or forbidden-construct rejection must carry a stable
`diagnostic_code`. The code is contractual; the message text is explanatory and
may evolve. Codes must be stable across whitespace, comments, and equivalent
formatting.

| Code family | Required use |
|---|---|
| `SRPL-LEX-*` | Invalid token, unterminated token, or unsupported character. |
| `SRPL-PARSE-SHAPE-*` | Invalid procedure shape, extra declaration, missing delimiter, empty result columns. |
| `SRPL-PARSE-TYPE-*` | Unsupported scalar, nullable/optional type syntax, float surface, invalid decimal/timestamp/text form. |
| `SRPL-PARSE-CARD-*` | Missing or invalid cardinality, ambiguous optional/nonempty form. |
| `SRPL-PARSE-BOUND-*` | Body operation count over 16 or any accepted surface that would exceed a parser bound. |
| `SRPL-PARSE-RESERVED-*` | Reserved `let`, `for_each`, absence keyword identifier, or future keyword surface. |
| `SRPL-FORBID-*` | No dynamic SQL, `select *`, recursion, random/non-determinism, network, filesystem, or loop rejection. |

Golden tests must assert code, phase, and source span. Tests may assert message
fragments only as non-contractual readability checks.

## Invariants

- No dynamic SQL text.
- No SELECT star.
- No implicit names.
- V0 has no positive loop construct; the operation list is bounded and unbounded loops are rejected.
- SRPL source has a deterministic parse tree.
- Parser diagnostics are stable and source-span based.
- Grammar productions must not create shape-changing result branches.
- A source unit declares at most one Procedure.
- A Procedure declares exactly one result stream in V0.
- Result stream cardinality is explicit and cannot be inferred from emitted
  values.
- Field absence is explicit by omission from V0: every field is required, and
  nullable spellings are rejected.
- Accepted grammar constructs must lower to typed AST variants, never stringly
  typed execution payloads.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

## State transitions

State transitions must be explicit. Invalid transitions return typed errors and emit trace evidence when they affect execution, storage, security, or recovery.

## Error model

| Error family | Use |
|---|---|
| ContractError | Invalid shape, incompatible hash, missing contract field. |
| PermissionError | Principal lacks required permission or surface scope. |
| ResourceError | Budget, quota, backpressure, or timeout failure. |
| TransactionError | Isolation, rollback, commit, or serialization failure. |
| StorageError | WAL, page, segment, manifest, or corruption failure. |
| SystemError | Internal condition requiring poison, rollback, forensic, or restore path. |

## Security model

Security-sensitive operations require admission through identity, principal, permission, policy, and audit checks before durable mutation or transaction creation.

## Observability

At minimum, implementations must emit trace evidence with:

```text
TraceId
InvocationId when applicable
CatalogVersion when applicable
PolicyVersion when applicable
Result
ErrorKind when applicable
```

## Recovery behavior

If this specification affects durable state, it must define how recovery replays, validates, rebuilds, or rejects the affected state.

## Compatibility

Changes are classified as:

| Change | Default status |
|---|---|
| Add optional field with explicit default | Additive |
| Add required field | Breaking |
| Change type or cardinality | Breaking |
| Change security requirement | Security-impact |
| Change recovery behavior | Breaking unless explicitly versioned |

## Tests

- Parse canonical signature-only examples.
- Parse canonical `body { read; assert; update; emit; raise; }` examples.
- Parse canonical `begin ensure; update set; return; end` examples.
- Parse every accepted cardinality spelling and verify canonical cardinality.
- Verify `optional one` / `optional_one` is result cardinality and not nullable
  column syntax.
- Reject `optional many`, `optional_many`, `nonempty one`, and
  `non_empty_one`.
- Reject ambiguous grammar, extra declarations, empty result streams, unsupported
  operations, and shape-changing branches.
- Reject `null`, `nullable`, `optional`, `option`, and `maybe` in type and
  identifier positions.
- Reject unbounded `while`, recursion, and any reserved `for_each` until a
  bounded loop spec exists.
- Reject dynamic text construction, raw SQL text, table-name strings, and
  `select *`.
- Reject external network and filesystem constructs.
- Verify the 16-operation body bound accepts 16 and rejects 17.
- Stable diagnostic code golden tests for lexing, parsing, type, cardinality,
  bound, reserved keyword, and forbidden-construct failures.
- Verify formatting-only source changes preserve AST semantics and diagnostic
  codes.

## Rejection criteria

- Reject `unbounded loop`.
- Reject `reserved for_each loop`.
- Reject `unstable diagnostic code`.
- Reject `shape-changing branch`.
- Reject `second procedure declaration`.
- Reject `missing explicit cardinality`.
- Reject `ambiguous cardinality spelling`.
- Reject `ambient nullable syntax`.
- Reject `nullable identifier`.
- Reject `dynamic SQL text`.
- Reject `raw executable text`.
- Reject `table name from string`.
- Reject `body operation count above 16`.
- Reject `external network call`.
- Reject `external filesystem call`.

## Acceptance summary

Owner crates: `andromeda-srpl-lexer`, `andromeda-srpl-parser`,
`andromeda-srpl-diagnostics`, and downstream SRPL lowering crates that consume
the typed AST.

Evidence: acceptance requires parser golden tests for canonical signatures,
bounded bodies, begin/end migration syntax, every accepted cardinality spelling,
UTF-8-safe source spans, the 16-operation body bound, and stable diagnostic code
snapshots for lexing, parsing, type, cardinality, bound, reserved-keyword, and
forbidden-construct failures.

Reject proof: acceptance requires explicit rejection tests for dynamic SQL text,
`select *`, raw executable text, table names from strings, ambient nullable
syntax, nullable identifiers, missing or ambiguous cardinality, second procedure
declarations, shape-changing result branches, `reserved for_each`, unbounded
loops, external network calls, and external filesystem calls.
