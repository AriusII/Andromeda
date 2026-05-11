use andromeda_error::AndromedaResult;
use andromeda_observability::{
    EventCorrelation, TraceId, TransactionPhaseCode, TransitionReasonCode,
};
use andromeda_observe::{
    CommitVisibleTrace, EventEmitter, EventSink, ExecutionTransitionTrace, RollbackDurableTrace,
    TraceEvent,
};
use andromeda_types::{InvocationId, TransactionId};
use andromeda_wal::Lsn;

pub(super) fn emit_commit_visible_event<S: EventSink>(
    emitter: &mut EventEmitter<S>,
    trace_id: TraceId,
    transaction_id: TransactionId,
    durable_lsn: Lsn,
    correlation: EventCorrelation,
) -> AndromedaResult<()> {
    emitter.emit(
        correlation,
        TraceEvent::CommitVisible(CommitVisibleTrace {
            trace_id,
            transaction_id,
            durable_commit_lsn: durable_lsn.get(),
        }),
    )?;
    Ok(())
}

pub(super) fn emit_rollback_durable_event<S: EventSink>(
    emitter: &mut EventEmitter<S>,
    trace_id: TraceId,
    transaction_id: TransactionId,
    durable_lsn: Lsn,
    correlation: EventCorrelation,
) -> AndromedaResult<()> {
    emitter.emit(
        correlation,
        TraceEvent::RollbackDurable(RollbackDurableTrace {
            trace_id,
            transaction_id,
            durable_rollback_lsn: durable_lsn.get(),
        }),
    )?;
    Ok(())
}

/// Emit a single `ExecutionTransitionTrace` event.
///
/// The correlation is passed by value to avoid requiring `Copy + Clone` in
/// all callers; `EventCorrelation` is `Copy` so the compiler handles the rest.
///
/// The `transaction_id` and `durable_lsn` carried by the trace payload are
/// mirrored into the `EventCorrelation` envelope automatically so that the
/// strict equality checks performed by `EventEnvelope` validation (ADR-0004
/// §6.2) always pass without requiring callers to pre-build per-step
/// correlations.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_execution_transition<S: EventSink>(
    emitter: &mut EventEmitter<S>,
    trace_id: TraceId,
    invocation_id: InvocationId,
    transaction_id: Option<TransactionId>,
    durable_lsn: Option<u64>,
    prev_phase: Option<TransactionPhaseCode>,
    next_phase: Option<TransactionPhaseCode>,
    reason_code: TransitionReasonCode,
    reason: &str,
    correlation: EventCorrelation,
) -> AndromedaResult<()> {
    // Mirror payload fields into the correlation envelope.  `EventEnvelope`
    // validation requires strict equality: when the trace payload carries a
    // `transaction_id` or `durable_lsn`, the envelope correlation MUST hold
    // the same value.  We keep the caller-supplied correlation for all other
    // fields (contract_hash, catalog_version, etc.) unchanged.
    let effective_correlation = EventCorrelation {
        transaction_id: transaction_id.or(correlation.transaction_id),
        durable_lsn: durable_lsn.or(correlation.durable_lsn),
        ..correlation
    };
    emitter.emit(
        effective_correlation,
        TraceEvent::ExecutionTransition(ExecutionTransitionTrace {
            trace_id,
            invocation_id,
            request_id: None,
            session_id: None,
            transaction_id,
            completion_code: None,
            prev_phase,
            next_phase,
            durable_lsn,
            reason_code,
            reason: reason.to_string(),
        }),
    )?;
    Ok(())
}

/// Emit the 8-transition sequence for a successful commit lifecycle.
///
/// Sequence (in emission order):
///
/// 1. `None → ADMITTED` — invocation admitted to runtime
/// 2. `ADMITTED → CONTRACT_BOUND` — contract binding verified
/// 3. `CONTRACT_BOUND → PERMISSION_CHECKED` — permissions authorized
/// 4. `PERMISSION_CHECKED → BUDGET_RESERVED` — IO budget admitted
/// 5. `BUDGET_RESERVED → TRANSACTION_OPENED` — transaction opened
/// 6. `TRANSACTION_OPENED → EXECUTING` — executing mutation payload
/// 7. `EXECUTING → COMMITTING` — committing to WAL
/// 8. `COMMITTING → COMMITTED` — durable WAL flush confirmed
///
/// Steps 1-5 carry no `transaction_id`; steps 6-8 carry the allocated id.
/// Step 8 requires a non-zero `durable_lsn`.
///
/// Returns the first error if any individual emission fails.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_invocation_commit_transition_sequence<S: EventSink>(
    emitter: &mut EventEmitter<S>,
    trace_id: TraceId,
    invocation_id: InvocationId,
    transaction_id: TransactionId,
    durable_lsn: u64,
    correlation: EventCorrelation,
) -> AndromedaResult<()> {
    // Steps 1-5: pre-transaction phases (no transaction_id yet).
    let steps: &[(Option<TransactionPhaseCode>, TransactionPhaseCode, &str)] = &[
        (
            None,
            TransactionPhaseCode::ADMITTED,
            "invocation admitted to runtime",
        ),
        (
            Some(TransactionPhaseCode::ADMITTED),
            TransactionPhaseCode::CONTRACT_BOUND,
            "contract binding verified",
        ),
        (
            Some(TransactionPhaseCode::CONTRACT_BOUND),
            TransactionPhaseCode::PERMISSION_CHECKED,
            "permissions authorized",
        ),
        (
            Some(TransactionPhaseCode::PERMISSION_CHECKED),
            TransactionPhaseCode::BUDGET_RESERVED,
            "io budget admitted",
        ),
        (
            Some(TransactionPhaseCode::BUDGET_RESERVED),
            TransactionPhaseCode::TRANSACTION_OPENED,
            "transaction opened",
        ),
    ];
    for &(prev, next, reason) in steps {
        emit_execution_transition(
            emitter,
            trace_id,
            invocation_id,
            None,
            None,
            prev,
            Some(next),
            TransitionReasonCode::NORMAL_PROGRESS,
            reason,
            correlation,
        )?;
    }

    // Step 6: TRANSACTION_OPENED → EXECUTING (transaction_id now known).
    emit_execution_transition(
        emitter,
        trace_id,
        invocation_id,
        Some(transaction_id),
        None,
        Some(TransactionPhaseCode::TRANSACTION_OPENED),
        Some(TransactionPhaseCode::EXECUTING),
        TransitionReasonCode::NORMAL_PROGRESS,
        "executing mutation payload",
        correlation,
    )?;

    // Step 7: EXECUTING → COMMITTING.
    emit_execution_transition(
        emitter,
        trace_id,
        invocation_id,
        Some(transaction_id),
        None,
        Some(TransactionPhaseCode::EXECUTING),
        Some(TransactionPhaseCode::COMMITTING),
        TransitionReasonCode::NORMAL_PROGRESS,
        "committing to WAL",
        correlation,
    )?;

    // Step 8: COMMITTING → COMMITTED (requires durable_lsn).
    emit_execution_transition(
        emitter,
        trace_id,
        invocation_id,
        Some(transaction_id),
        Some(durable_lsn),
        Some(TransactionPhaseCode::COMMITTING),
        Some(TransactionPhaseCode::COMMITTED),
        TransitionReasonCode::DURABLE_WAL_FLUSH,
        "durable WAL flush confirmed",
        correlation,
    )?;

    Ok(())
}

/// Emit the 8-transition sequence for a business-failure rollback lifecycle.
///
/// Sequence (in emission order):
///
/// 1-5. Same pre-transaction steps as the commit sequence.
/// 6. `TRANSACTION_OPENED → FAILED` — business validation failure
/// 7. `FAILED → ROLLING_BACK` — rolling back after business failure
/// 8. `ROLLING_BACK → ROLLED_BACK` — durable WAL rollback confirmed
///
/// Returns the first error if any individual emission fails.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_invocation_rollback_transition_sequence<S: EventSink>(
    emitter: &mut EventEmitter<S>,
    trace_id: TraceId,
    invocation_id: InvocationId,
    transaction_id: TransactionId,
    durable_lsn: u64,
    correlation: EventCorrelation,
) -> AndromedaResult<()> {
    // Steps 1-5: pre-transaction phases (no transaction_id).
    let steps: &[(Option<TransactionPhaseCode>, TransactionPhaseCode, &str)] = &[
        (
            None,
            TransactionPhaseCode::ADMITTED,
            "invocation admitted to runtime",
        ),
        (
            Some(TransactionPhaseCode::ADMITTED),
            TransactionPhaseCode::CONTRACT_BOUND,
            "contract binding verified",
        ),
        (
            Some(TransactionPhaseCode::CONTRACT_BOUND),
            TransactionPhaseCode::PERMISSION_CHECKED,
            "permissions authorized",
        ),
        (
            Some(TransactionPhaseCode::PERMISSION_CHECKED),
            TransactionPhaseCode::BUDGET_RESERVED,
            "io budget admitted",
        ),
        (
            Some(TransactionPhaseCode::BUDGET_RESERVED),
            TransactionPhaseCode::TRANSACTION_OPENED,
            "transaction opened",
        ),
    ];
    for &(prev, next, reason) in steps {
        emit_execution_transition(
            emitter,
            trace_id,
            invocation_id,
            None,
            None,
            prev,
            Some(next),
            TransitionReasonCode::NORMAL_PROGRESS,
            reason,
            correlation,
        )?;
    }

    // Step 6: TRANSACTION_OPENED → FAILED.
    emit_execution_transition(
        emitter,
        trace_id,
        invocation_id,
        Some(transaction_id),
        None,
        Some(TransactionPhaseCode::TRANSACTION_OPENED),
        Some(TransactionPhaseCode::FAILED),
        TransitionReasonCode::EXECUTOR_FAILURE,
        "business validation failure",
        correlation,
    )?;

    // Step 7: FAILED → ROLLING_BACK.
    emit_execution_transition(
        emitter,
        trace_id,
        invocation_id,
        Some(transaction_id),
        None,
        Some(TransactionPhaseCode::FAILED),
        Some(TransactionPhaseCode::ROLLING_BACK),
        TransitionReasonCode::EXECUTOR_FAILURE,
        "rolling back after business failure",
        correlation,
    )?;

    // Step 8: ROLLING_BACK → ROLLED_BACK (requires durable_lsn).
    emit_execution_transition(
        emitter,
        trace_id,
        invocation_id,
        Some(transaction_id),
        Some(durable_lsn),
        Some(TransactionPhaseCode::ROLLING_BACK),
        Some(TransactionPhaseCode::ROLLED_BACK),
        TransitionReasonCode::DURABLE_WAL_FLUSH,
        "durable WAL rollback confirmed",
        correlation,
    )?;

    Ok(())
}
