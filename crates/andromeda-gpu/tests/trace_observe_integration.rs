#![cfg(feature = "trace")]

//! Feature-gated observe integration tests for GPU execution traces.

use andromeda_gpu::{
    budget::{GpuBudget, GpuBudgetRequest},
    fallback::ValidationState,
    job::{FallbackReason, GpuJobClass},
    observe_integration::{GpuTraceCorrelation, gpu_execution_envelope},
    trace::{GpuBudgetEvidence, GpuExecutionResult, GpuExecutionTrace},
};
use andromeda_observability::EventId;
use andromeda_observe::{GpuExecutionJobClass, GpuExecutionOutcome, TraceEvent};
use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash, RequestId, SessionId};

fn budget_evidence() -> GpuBudgetEvidence {
    GpuBudgetEvidence::new(
        GpuBudgetRequest {
            memory_bytes: 1_024,
            time_ms: 10,
            transfer_bytes: 2_048,
        },
        GpuBudget::new(4_096, 20, 8_192).unwrap(),
    )
}

fn successful_trace(trace_id: u128) -> GpuExecutionTrace {
    GpuExecutionTrace::new(GpuJobClass::Analytics, trace_id, 100)
        .finish(200, GpuExecutionResult::Success)
        .with_validation_state(ValidationState::Validated)
        .with_budget_evidence(budget_evidence())
}

#[test]
fn gpu_observe_emission_accepts_optional_request_session_contract_catalog() {
    let envelope = gpu_execution_envelope(
        EventId::new(1),
        GpuTraceCorrelation {
            request_id: Some(RequestId::new(10)),
            session_id: Some(SessionId::new(20)),
            contract_hash: Some(ContractHash::test_vector(0xab)),
            catalog_version: Some(CatalogVersion::new(30)),
            catalog_object_id: Some(CatalogObjectId::new(40)),
        },
        &successful_trace(99),
    )
    .expect("paired optional correlation should be valid for GPU evidence");

    assert_eq!(envelope.trace_id.get(), 99);
    match envelope.event {
        TraceEvent::GpuExecution(trace) => {
            assert_eq!(trace.job_class, GpuExecutionJobClass::Analytics);
            assert_eq!(trace.outcome, GpuExecutionOutcome::Success);
            assert_eq!(trace.started_at_unix_ms, 100);
            assert_eq!(trace.finished_at_unix_ms, 200);
        },
        other => panic!("expected GPU execution event, got {other:?}"),
    }
}

#[test]
fn gpu_observe_emission_rejects_unpaired_or_zero_optional_correlation() {
    let trace = successful_trace(100);

    let unpaired_request = gpu_execution_envelope(
        EventId::new(2),
        GpuTraceCorrelation {
            request_id: Some(RequestId::new(10)),
            session_id: None,
            contract_hash: None,
            catalog_version: None,
            catalog_object_id: None,
        },
        &trace,
    )
    .unwrap_err();
    assert!(
        unpaired_request
            .message()
            .contains("request_id and session_id")
    );

    let unpaired_contract = gpu_execution_envelope(
        EventId::new(3),
        GpuTraceCorrelation {
            request_id: None,
            session_id: None,
            contract_hash: Some(ContractHash::test_vector(0xcd)),
            catalog_version: None,
            catalog_object_id: None,
        },
        &trace,
    )
    .unwrap_err();
    assert!(
        unpaired_contract
            .message()
            .contains("contract_hash and catalog_version")
    );

    let zero_request = gpu_execution_envelope(
        EventId::new(4),
        GpuTraceCorrelation {
            request_id: Some(RequestId::new(0)),
            session_id: Some(SessionId::new(1)),
            contract_hash: None,
            catalog_version: None,
            catalog_object_id: None,
        },
        &trace,
    )
    .unwrap_err();
    assert!(zero_request.message().contains("request_id correlation"));
}

#[test]
fn gpu_observe_emission_rejects_sensitive_markers_in_text() {
    let trace = GpuExecutionTrace::new(GpuJobClass::Statistics, 101, 100)
        .finish(200, GpuExecutionResult::Failed("token=abc".to_owned()))
        .with_validation_state(ValidationState::NotValidated)
        .with_fallback_reason(FallbackReason::GpuComputationFailed)
        .with_budget_evidence(budget_evidence());

    let rejected =
        gpu_execution_envelope(EventId::new(5), GpuTraceCorrelation::empty(), &trace).unwrap_err();

    assert!(rejected.message().contains("must not include secrets"));
}
