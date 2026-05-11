//! Tests bounded GPU trace failure/rejection reason handling.

use andromeda_gpu::{
    fallback::ValidationState,
    job::GpuJobClass,
    trace::{GpuExecutionResult, GpuExecutionTrace},
};

#[test]
fn short_failure_and_rejection_reasons_are_preserved() {
    let trace = GpuExecutionTrace::new(GpuJobClass::Analytics, 5, 10)
        .finish(20, GpuExecutionResult::Failed("gpu timeout".to_owned()))
        .with_validation_state(ValidationState::Rejected("mismatch".to_owned()));

    assert_eq!(
        trace.result,
        GpuExecutionResult::Failed("gpu timeout".to_owned())
    );
    assert_eq!(
        trace.validation_state,
        ValidationState::Rejected("mismatch".to_owned())
    );
}

#[test]
fn long_failure_and_rejection_reasons_are_bounded() {
    let long_reason = "s".repeat(4_096);
    let trace = GpuExecutionTrace::new(GpuJobClass::Statistics, 7, 100)
        .finish(200, GpuExecutionResult::Failed(long_reason.clone()))
        .with_validation_state(ValidationState::Rejected(long_reason));

    let failure_reason = match trace.result {
        GpuExecutionResult::Failed(reason) => reason,
        other => panic!("expected Failed, got {other:?}"),
    };
    assert_eq!(failure_reason.len(), GpuExecutionTrace::MAX_REASON_BYTES);
    assert!(failure_reason.ends_with("..."));

    let rejected_reason = match trace.validation_state {
        ValidationState::Rejected(reason) => reason,
        other => panic!("expected Rejected, got {other:?}"),
    };
    assert_eq!(rejected_reason.len(), GpuExecutionTrace::MAX_REASON_BYTES);
    assert!(rejected_reason.ends_with("..."));
}
