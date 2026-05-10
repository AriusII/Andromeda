use andromeda_error::AndromedaResult;
use andromeda_observability::{EventCorrelation, TraceId};
use andromeda_observe::{
    CommitVisibleTrace, EventEmitter, EventSink, RollbackDurableTrace, TraceEvent,
};
use andromeda_types::TransactionId;
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
