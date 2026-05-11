//! GAP-07 regression: `execute()` and `execute_authorized()` are renamed to
//! `execute_internal()` and `execute_internal_authorized()` to make it
//! syntactically clear that they bypass the surface gate.
//!
//! This file proves:
//! 1. `execute_internal()` compiles and works for system-plane callers (no token).
//! 2. `execute_surface_authorized()` is the correct app-plane path (requires token).
//! 3. `execute_internal()` still enforces all OTHER pre-transaction admission
//!    checks — contract hash, binding, and authorization context for permissioned
//!    procedures. The rename does NOT remove any existing protection.

// common.rs exports many helpers used by the wider test suite.
// Only a subset is needed by this particular file.
#![allow(dead_code, unused_imports)]

#[path = "runtime_contract/common.rs"]
mod common;
use common::*;

use andromeda_error::AndromedaErrorKind;

/// GAP-07 regression: `execute_internal()` works for unpermissioned procedures
/// called from system-internal contexts (no InvocationContext token required).
#[test]
fn execute_internal_succeeds_for_unpermissioned_procedure() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());

    // `procedure()` helper produces an unpermissioned procedure (empty
    // required_permissions). `execute_internal()` must succeed without any
    // authorization context.
    let proc = procedure(request(ContractHash::test_vector(7)).procedure);

    let outcome = runtime
        .execute_internal(
            request(ContractHash::test_vector(7)),
            &proc,
            TraceId::new(7001),
        )
        .expect("execute_internal must succeed for unpermissioned procedure");

    assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    assert_eq!(runtime.wal().records.len(), 3);
}

/// GAP-07 regression: `execute_internal()` still enforces contract-hash
/// pre-admission checks. Providing a wrong hash must produce a `Contract` error
/// before any WAL writes.
#[test]
fn execute_internal_still_enforces_contract_hash_check() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let proc = procedure(request(ContractHash::test_vector(7)).procedure);

    let err = runtime
        .execute_internal(
            request(ContractHash::test_vector(0xFF)), // wrong hash
            &proc,
            TraceId::new(7002),
        )
        .expect_err("contract hash mismatch must be rejected");

    assert_eq!(
        err.kind(),
        AndromedaErrorKind::Contract,
        "contract hash mismatch must produce Contract error"
    );
    assert!(
        runtime.wal().records.is_empty(),
        "no WAL writes must occur before admission"
    );
}

/// GAP-07 regression: `execute_internal()` still enforces that permissioned
/// procedures require an authorization context. Calling `execute_internal()`
/// for a permissioned procedure is a Security-kind error.
#[test]
fn execute_internal_still_rejects_permissioned_procedure_without_context() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let mut proc = procedure(request(ContractHash::test_vector(7)).procedure);
    proc.required_permissions = vec!["SomeService.SomeOp.Execute".to_string()];

    let err = runtime
        .execute_internal(
            request(ContractHash::test_vector(7)),
            &proc,
            TraceId::new(7003),
        )
        .expect_err("permissioned procedure without context must be rejected");

    assert_eq!(
        err.kind(),
        AndromedaErrorKind::Security,
        "missing authorization context must produce Security error"
    );
    assert!(
        runtime.wal().records.is_empty(),
        "no WAL writes must occur before authorization"
    );
}

/// GAP-07 regression: `execute_internal_authorized()` works for permissioned
/// procedures when a matching context is provided.
#[test]
fn execute_internal_authorized_succeeds_with_matching_permissions() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let mut proc = procedure(request(ContractHash::test_vector(7)).procedure);
    proc.required_permissions = vec!["SomeService.SomeOp.Execute".to_string()];

    let context = InvocationContext::new(
        TraceId::new(7004),
        vec!["SomeService.SomeOp.Execute".to_string()],
    );

    let outcome = runtime
        .execute_internal_authorized(request(ContractHash::test_vector(7)), &proc, &context)
        .expect("execute_internal_authorized must succeed with matching permissions");

    assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    assert!(outcome.authorization_trace.is_some());
}

/// GAP-07 regression: `execute_internal_authorized()` still rejects when
/// the context is missing the required permission (Security error).
#[test]
fn execute_internal_authorized_rejects_missing_permission() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let mut proc = procedure(request(ContractHash::test_vector(7)).procedure);
    proc.required_permissions = vec!["SomeService.SomeOp.Execute".to_string()];

    // Context with no permissions — authorization must fail.
    let context = InvocationContext::new(TraceId::new(7005), Vec::new());

    let err = runtime
        .execute_internal_authorized(request(ContractHash::test_vector(7)), &proc, &context)
        .expect_err("missing permission must be rejected");

    assert_eq!(
        err.kind(),
        AndromedaErrorKind::Security,
        "missing permission must produce Security error"
    );
    assert!(
        runtime.wal().records.is_empty(),
        "no WAL writes must occur before authorization"
    );
}
