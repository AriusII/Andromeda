use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_plan_cache::{PlanCacheKey, PlanClass, PlanShapeFingerprint};
use andromeda_procedure_store::{
    InvocationRuntimeRecord, ProcedureRuntimeCounters, ProcedureRuntimePlanId,
    ProcedureRuntimeStatus,
};
use andromeda_time::{Clock, EngineTimestamp, SystemClock};
use andromeda_transaction_log::TX_COMMIT_PAYLOAD_LEN;

use crate::{InvocationRequest, RollbackCause, local::types::LocalProcedure};

#[derive(Debug, Clone, Copy)]
pub(super) struct RuntimeRecordSpan {
    started_at: EngineTimestamp,
}

impl RuntimeRecordSpan {
    pub(super) fn started() -> Self {
        Self {
            started_at: nonzero_runtime_timestamp(SystemClock.now()),
        }
    }

    fn started_at(self) -> EngineTimestamp {
        self.started_at
    }

    fn completed_at(self) -> EngineTimestamp {
        nonzero_runtime_timestamp(SystemClock.now()).max(self.started_at)
    }
}

pub(super) fn committed_runtime_record(
    request: &InvocationRequest,
    procedure: &LocalProcedure,
    span: RuntimeRecordSpan,
) -> AndromedaResult<InvocationRuntimeRecord> {
    let (plan_key, plan_id) = singleton_runtime_plan(procedure)?;
    InvocationRuntimeRecord::new(
        request.invocation_id,
        procedure.contract_binding,
        span.started_at(),
        span.completed_at(),
        Some(plan_key),
        Some(plan_id),
        ProcedureRuntimeCounters::new(
            procedure
                .result_metadata
                .row_count_exact
                .unwrap_or_default(),
            procedure.rows_affected,
            committed_wal_payload_bytes(procedure),
            0,
        ),
        ProcedureRuntimeStatus::Committed,
        None,
    )
}

pub(super) fn rolled_back_runtime_record(
    request: &InvocationRequest,
    procedure: &LocalProcedure,
    cause: RollbackCause,
    rollback_payload_len: usize,
    span: RuntimeRecordSpan,
) -> AndromedaResult<InvocationRuntimeRecord> {
    let (plan_key, plan_id) = singleton_runtime_plan(procedure)?;
    InvocationRuntimeRecord::new(
        request.invocation_id,
        procedure.contract_binding,
        span.started_at(),
        span.completed_at(),
        Some(plan_key),
        Some(plan_id),
        ProcedureRuntimeCounters::new(
            procedure
                .result_metadata
                .row_count_exact
                .unwrap_or_default(),
            0,
            rolled_back_wal_payload_bytes(rollback_payload_len),
            0,
        ),
        ProcedureRuntimeStatus::RolledBack,
        Some(rollback_error_kind(cause)),
    )
}

fn singleton_runtime_plan(
    procedure: &LocalProcedure,
) -> AndromedaResult<(PlanCacheKey, ProcedureRuntimePlanId)> {
    let key = PlanCacheKey::build(
        procedure.contract_binding,
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .map_err(|error| {
        AndromedaError::new(
            AndromedaErrorKind::Contract,
            format!("local Procedure runtime record cannot derive singleton plan key: {error}"),
        )
    })?;
    Ok((key, ProcedureRuntimePlanId::from_plan_cache_key(&key)))
}

fn committed_wal_payload_bytes(procedure: &LocalProcedure) -> u64 {
    let mutation_bytes = if !procedure.mutation_payload.is_empty() {
        procedure.mutation_payload.len() as u64
    } else {
        0
    };
    b"tx-begin".len() as u64 + mutation_bytes + TX_COMMIT_PAYLOAD_LEN as u64
}

fn rolled_back_wal_payload_bytes(rollback_payload_len: usize) -> u64 {
    b"tx-begin".len() as u64 + rollback_payload_len as u64
}

fn rollback_error_kind(cause: RollbackCause) -> AndromedaErrorKind {
    match cause {
        RollbackCause::Direct => AndromedaErrorKind::Transaction,
        RollbackCause::BusinessFailure => AndromedaErrorKind::Execution,
        RollbackCause::Poison => AndromedaErrorKind::Internal,
    }
}

fn nonzero_runtime_timestamp(timestamp: EngineTimestamp) -> EngineTimestamp {
    if timestamp.is_zero() {
        EngineTimestamp::from_unix_millis(1)
    } else {
        timestamp
    }
}
