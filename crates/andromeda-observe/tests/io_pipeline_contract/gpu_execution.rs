use crate::support::*;

#[test]
fn gpu_execution_event_is_typed_and_gpu_family() {
    let envelope = EventEnvelope::new(
        EventId::new(31),
        EventCorrelation::empty(),
        TraceEvent::GpuExecution(GpuExecutionTraceEvent {
            trace_id: TraceId::new(901),
            job_class: GpuExecutionJobClass::Analytics,
            started_at_unix_ms: 10,
            finished_at_unix_ms: 20,
            outcome: GpuExecutionOutcome::Success,
            validation: GpuValidationOutcome::Validated,
            fallback_reason: None,
            budget_evidence: GpuBudgetTraceEvidence {
                requested_memory_bytes: 2_048,
                requested_time_ms: 20,
                requested_transfer_bytes: 4_096,
                budget_memory_bytes: 4_096,
                budget_time_ms: 40,
                budget_transfer_bytes: 8_192,
            },
        }),
    )
    .expect("GPU execution trace should be accepted as non-authoritative evidence");

    assert_eq!(
        envelope.event.kind(),
        CriticalDecisionKind::GpuExecutionTrace
    );
    assert_eq!(
        envelope.event.trace_event_family(),
        TraceEventFamily::Gpu,
        "GPU execution evidence belongs to the GPU family, not durability or security"
    );
}

#[test]
fn gpu_execution_event_rejects_c5_correlation() {
    let event = TraceEvent::GpuExecution(GpuExecutionTraceEvent {
        trace_id: TraceId::new(902),
        job_class: GpuExecutionJobClass::Statistics,
        started_at_unix_ms: 10,
        finished_at_unix_ms: 20,
        outcome: GpuExecutionOutcome::Success,
        validation: GpuValidationOutcome::Validated,
        fallback_reason: None,
        budget_evidence: GpuBudgetTraceEvidence {
            requested_memory_bytes: 2_048,
            requested_time_ms: 20,
            requested_transfer_bytes: 4_096,
            budget_memory_bytes: 4_096,
            budget_time_ms: 40,
            budget_transfer_bytes: 8_192,
        },
    });

    let rejected = EventEnvelope::new(
        EventId::new(32),
        EventCorrelation {
            transaction_id: Some(andromeda_types::TransactionId::new(1)),
            durable_lsn: None,
            ..EventCorrelation::empty()
        },
        event,
    )
    .unwrap_err();

    assert!(rejected.message().contains("transaction or durable LSN"));
}
