//! End-to-end acceptance tests for `TransactionTransitionTrace` emission from
//! `TransactionManager`.
//!
//! These tests exercise the full lifecycle paths through `TransactionManager`
//! with an `InMemoryTransitionSink` attached, asserting that:
//!
//! - The correct number of traces is emitted per path.
//! - Each trace has the expected `(prev_phase, next_phase, reason_code)`.
//! - Durable LSN is present and correct on terminal `Committed`/`RolledBack`
//!   traces.
//! - Every emitted trace passes `TransactionTransitionTrace::validate()`.
//! - Failed state-machine transitions (invalid operations) do **not** push
//!   any trace into the sink.

use std::sync::Arc;

use andromeda_observability::{
    TraceId, TransactionPhaseCode, TransactionTransitionTrace, TransitionReasonCode,
};
use andromeda_transaction::{
    InMemoryTransitionSink, TransactionManager, TransactionTransitionCorrelation,
};
use andromeda_types::{InvocationId, RequestId, SessionId};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn manager_with_sink(sink: Arc<InMemoryTransitionSink>) -> TransactionManager {
    TransactionManager::new().with_transition_sink(sink)
}

fn assert_trace(
    trace: &TransactionTransitionTrace,
    prev: TransactionPhaseCode,
    next: TransactionPhaseCode,
    reason: TransitionReasonCode,
) {
    assert_eq!(
        trace.prev_phase, prev,
        "expected prev={prev:?}, got {:?}",
        trace.prev_phase
    );
    assert_eq!(
        trace.next_phase, next,
        "expected next={next:?}, got {:?}",
        trace.next_phase
    );
    assert_eq!(
        trace.reason_code, reason,
        "expected reason={reason:?}, got {:?}",
        trace.reason_code
    );
    trace
        .validate()
        .expect("emitted trace must pass validate()");
}

// ---------------------------------------------------------------------------
// Happy path: begin → request_commit → commit_durable → dispose
// ---------------------------------------------------------------------------

#[test]
fn happy_commit_path_emits_four_traces_with_correct_metadata() {
    let sink = Arc::new(InMemoryTransitionSink::new());
    let mgr = manager_with_sink(Arc::clone(&sink));

    let id = mgr.begin().unwrap();
    mgr.request_commit(id).unwrap();
    mgr.commit_durable(id, 42).unwrap();
    mgr.dispose(id).unwrap();

    let traces = sink.drain();
    assert_eq!(
        traces.len(),
        4,
        "expected exactly 4 traces for happy commit path"
    );

    assert_trace(
        &traces[0],
        TransactionPhaseCode::CREATED,
        TransactionPhaseCode::ACTIVE,
        TransitionReasonCode::NORMAL_PROGRESS,
    );
    assert_trace(
        &traces[1],
        TransactionPhaseCode::ACTIVE,
        TransactionPhaseCode::COMMITTING,
        TransitionReasonCode::NORMAL_PROGRESS,
    );
    assert_trace(
        &traces[2],
        TransactionPhaseCode::COMMITTING,
        TransactionPhaseCode::COMMITTED,
        TransitionReasonCode::DURABLE_WAL_FLUSH,
    );
    // durable_lsn must be present and equal to 42 on the commit trace
    assert_eq!(
        traces[2].durable_lsn,
        Some(42),
        "commit trace must carry the durable LSN"
    );
    assert_trace(
        &traces[3],
        TransactionPhaseCode::COMMITTED,
        TransactionPhaseCode::DISPOSED,
        TransitionReasonCode::NORMAL_PROGRESS,
    );
}

// ---------------------------------------------------------------------------
// Rollback path: begin → request_rollback → rollback_durable → dispose
// ---------------------------------------------------------------------------

#[test]
fn rollback_path_emits_four_traces_with_correct_metadata() {
    let sink = Arc::new(InMemoryTransitionSink::new());
    let mgr = manager_with_sink(Arc::clone(&sink));

    let id = mgr.begin().unwrap();
    mgr.request_rollback(id).unwrap();
    mgr.rollback_durable(id, 99).unwrap();
    mgr.dispose(id).unwrap();

    let traces = sink.drain();
    assert_eq!(
        traces.len(),
        4,
        "expected exactly 4 traces for rollback path"
    );

    assert_trace(
        &traces[0],
        TransactionPhaseCode::CREATED,
        TransactionPhaseCode::ACTIVE,
        TransitionReasonCode::NORMAL_PROGRESS,
    );
    assert_trace(
        &traces[1],
        TransactionPhaseCode::ACTIVE,
        TransactionPhaseCode::ROLLING_BACK,
        TransitionReasonCode::CALLER_ROLLBACK,
    );
    assert_trace(
        &traces[2],
        TransactionPhaseCode::ROLLING_BACK,
        TransactionPhaseCode::ROLLED_BACK,
        TransitionReasonCode::DURABLE_WAL_FLUSH,
    );
    assert_eq!(
        traces[2].durable_lsn,
        Some(99),
        "rolled-back trace must carry the durable rollback LSN"
    );
    assert_trace(
        &traces[3],
        TransactionPhaseCode::ROLLED_BACK,
        TransactionPhaseCode::DISPOSED,
        TransitionReasonCode::NORMAL_PROGRESS,
    );
}

// ---------------------------------------------------------------------------
// Poison path: begin → poison → request_rollback → rollback_durable → dispose
// ---------------------------------------------------------------------------

#[test]
fn poison_path_emits_five_traces_with_correct_metadata() {
    let sink = Arc::new(InMemoryTransitionSink::new());
    let mgr = manager_with_sink(Arc::clone(&sink));

    let id = mgr.begin().unwrap();
    mgr.poison(id).unwrap();
    mgr.request_rollback(id).unwrap();
    mgr.rollback_durable(id, 7).unwrap();
    mgr.dispose(id).unwrap();

    let traces = sink.drain();
    assert_eq!(traces.len(), 5, "expected exactly 5 traces for poison path");

    assert_trace(
        &traces[0],
        TransactionPhaseCode::CREATED,
        TransactionPhaseCode::ACTIVE,
        TransitionReasonCode::NORMAL_PROGRESS,
    );
    assert_trace(
        &traces[1],
        TransactionPhaseCode::ACTIVE,
        TransactionPhaseCode::POISONED,
        TransitionReasonCode::POISON,
    );
    assert_trace(
        &traces[2],
        TransactionPhaseCode::POISONED,
        TransactionPhaseCode::ROLLING_BACK,
        // Poison predecessor is treated like CALLER_ROLLBACK (not
        // EXECUTOR_FAILURE, which is reserved for the `fail()` path).
        TransitionReasonCode::CALLER_ROLLBACK,
    );
    assert_trace(
        &traces[3],
        TransactionPhaseCode::ROLLING_BACK,
        TransactionPhaseCode::ROLLED_BACK,
        TransitionReasonCode::DURABLE_WAL_FLUSH,
    );
    assert_eq!(traces[3].durable_lsn, Some(7));
    assert_trace(
        &traces[4],
        TransactionPhaseCode::ROLLED_BACK,
        TransactionPhaseCode::DISPOSED,
        TransitionReasonCode::NORMAL_PROGRESS,
    );
}

// ---------------------------------------------------------------------------
// Fail path: begin → fail → request_rollback → rollback_durable → dispose
// ---------------------------------------------------------------------------

#[test]
fn fail_path_reason_code_is_executor_failure() {
    let sink = Arc::new(InMemoryTransitionSink::new());
    let mgr = manager_with_sink(Arc::clone(&sink));

    let id = mgr.begin().unwrap();
    mgr.fail(id).unwrap();
    mgr.request_rollback(id).unwrap();
    mgr.rollback_durable(id, 55).unwrap();
    mgr.dispose(id).unwrap();

    let traces = sink.drain();
    assert_eq!(traces.len(), 5);

    // fail trace
    assert_trace(
        &traces[1],
        TransactionPhaseCode::ACTIVE,
        TransactionPhaseCode::FAILED,
        TransitionReasonCode::EXECUTOR_FAILURE,
    );
    // request_rollback from Failed state → EXECUTOR_FAILURE
    assert_trace(
        &traces[2],
        TransactionPhaseCode::FAILED,
        TransactionPhaseCode::ROLLING_BACK,
        TransitionReasonCode::EXECUTOR_FAILURE,
    );
}

// ---------------------------------------------------------------------------
// Negative: invalid transition must NOT push any trace
// ---------------------------------------------------------------------------

#[test]
fn invalid_transition_does_not_emit_trace() {
    let sink = Arc::new(InMemoryTransitionSink::new());
    let mgr = manager_with_sink(Arc::clone(&sink));

    let id = mgr.begin().unwrap();
    // begin emits 1 trace
    assert_eq!(sink.len(), 1);

    // Attempt commit_durable while Active (not Committing) — must fail
    let err = mgr.commit_durable(id, 1);
    assert!(err.is_err(), "commit_durable while Active must be rejected");

    // Sink must still contain exactly 1 trace (the begin trace only)
    assert_eq!(
        sink.len(),
        1,
        "failed transition must not push a trace into the sink"
    );
}

// ---------------------------------------------------------------------------
// Correlation: begin_with_correlation propagates ids through all traces
// ---------------------------------------------------------------------------

#[test]
fn begin_with_correlation_propagates_ids_through_all_traces() {
    let sink = Arc::new(InMemoryTransitionSink::new());
    let mgr = manager_with_sink(Arc::clone(&sink));

    let correlation = TransactionTransitionCorrelation {
        invocation_id: Some(InvocationId::new(42)),
        request_id: Some(RequestId::new(7)),
        session_id: Some(SessionId::new(99)),
    };
    let trace_id = TraceId::new(1001);

    let id = mgr.begin_with_correlation(correlation, trace_id).unwrap();
    mgr.request_commit(id).unwrap();
    mgr.commit_durable(id, 500).unwrap();
    mgr.dispose(id).unwrap();

    let traces = sink.drain();
    assert_eq!(traces.len(), 4);

    for t in &traces {
        assert_eq!(
            t.trace_id, trace_id,
            "all traces must carry the root trace_id"
        );
        assert_eq!(t.invocation_id, Some(InvocationId::new(42)));
        assert_eq!(t.request_id, Some(RequestId::new(7)));
        assert_eq!(t.session_id, Some(SessionId::new(99)));
        t.validate().expect("correlated trace must validate");
    }
}

// ---------------------------------------------------------------------------
// Multiple concurrent transactions: sinks receive all traces correctly
// ---------------------------------------------------------------------------

#[test]
fn multiple_transactions_emit_independent_traces() {
    let sink = Arc::new(InMemoryTransitionSink::new());
    let mgr = manager_with_sink(Arc::clone(&sink));

    let a = mgr.begin().unwrap();
    let b = mgr.begin().unwrap();

    mgr.request_commit(a).unwrap();
    mgr.commit_durable(a, 10).unwrap();
    mgr.dispose(a).unwrap();

    mgr.request_rollback(b).unwrap();
    mgr.rollback_durable(b, 20).unwrap();
    mgr.dispose(b).unwrap();

    let traces = sink.drain();
    // 4 traces for a + 4 traces for b
    assert_eq!(traces.len(), 8);

    // Every trace must validate independently
    for t in &traces {
        t.validate().expect("each trace must pass validate()");
    }
}
