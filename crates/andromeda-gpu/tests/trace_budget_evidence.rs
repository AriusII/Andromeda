//! Verifies GPU trace budget evidence is populated across prototype paths.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_gpu::{
    budget::{GpuBudget, GpuBudgetRequest},
    fallback::{CpuFallback, ValidationState},
    job::{FallbackReason, GpuJobClass},
    kill_switch::KillSwitchHandle,
    policy::GpuProfile,
    prototypes::{
        analytics::{AnalyticsInput, AnalyticsOutput, GpuAnalyticsBindingContext},
        permit::{AnalyticsExecutionPermit, StatsExecutionPermit},
        stats::{GpuStatsBindingContext, StatsInput},
    },
    trace::{GpuBudgetEvidence, GpuExecutionResult, GpuExecutionTrace},
    validation::{GpuStatsValidationGate, Histogram},
};

struct StatsCpu;

impl CpuFallback for StatsCpu {
    type Input = StatsInput;
    type Output = Histogram;

    fn execute(&self, _input: &StatsInput) -> AndromedaResult<Histogram> {
        Ok(Histogram {
            buckets: vec![100, 200, 300],
        })
    }
}

struct AnalyticsCpu;

impl CpuFallback for AnalyticsCpu {
    type Input = AnalyticsInput;
    type Output = AnalyticsOutput;

    fn execute(&self, _input: &AnalyticsInput) -> AndromedaResult<AnalyticsOutput> {
        Ok(AnalyticsOutput {
            row_count: 1_000,
            digest: 42,
        })
    }
}

fn expected_stats_budget(input: &StatsInput, budget: GpuBudget) -> GpuBudgetEvidence {
    GpuBudgetEvidence::new(
        GpuBudgetRequest {
            memory_bytes: 4_096 + u64::from(input.column_id) * 256,
            time_ms: 10,
            transfer_bytes: 4_096 + (input.snapshot_lsn % 4_096),
        },
        budget,
    )
}

fn expected_analytics_budget(input: &AnalyticsInput, budget: GpuBudget) -> GpuBudgetEvidence {
    let column_count = (input.column_ids.len() as u64).max(1);
    GpuBudgetEvidence::new(
        GpuBudgetRequest {
            memory_bytes: 2_048 + column_count * 512,
            time_ms: 25 + column_count * 5,
            transfer_bytes: 4_096 + column_count * 1_024,
        },
        budget,
    )
}

#[test]
fn stats_success_trace_carries_budget_evidence() {
    let budget = GpuBudget::new(10_000_000, 10_000, 10_000_000).unwrap();
    let context = GpuStatsBindingContext {
        job: GpuJobClass::Statistics,
        kill_switch: KillSwitchHandle::new(),
        budget,
        validation_gate: GpuStatsValidationGate::new(5.0).unwrap(),
        cpu_fallback: StatsCpu,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };
    let input = StatsInput {
        snapshot_lsn: 16,
        column_id: 2,
    };
    let permit = StatsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    let (_, trace) = context
        .execute(&permit, &input, |_| {
            Ok(Histogram {
                buckets: vec![100, 200, 300],
            })
        })
        .unwrap();

    assert_eq!(trace.result, GpuExecutionResult::Success);
    assert_eq!(trace.validation_state, ValidationState::Validated);
    assert_eq!(
        trace.budget_evidence,
        Some(expected_stats_budget(&input, budget))
    );
}

#[test]
fn stats_validation_fallback_trace_carries_budget_evidence() {
    let budget = GpuBudget::new(10_000_000, 10_000, 10_000_000).unwrap();
    let context = GpuStatsBindingContext {
        job: GpuJobClass::Statistics,
        kill_switch: KillSwitchHandle::new(),
        budget,
        validation_gate: GpuStatsValidationGate::new(1.0).unwrap(),
        cpu_fallback: StatsCpu,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };
    let input = StatsInput {
        snapshot_lsn: 99,
        column_id: 1,
    };
    let permit = StatsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    let (_, trace) = context
        .execute(&permit, &input, |_| {
            Ok(Histogram {
                buckets: vec![120, 200, 300],
            })
        })
        .unwrap();

    assert!(matches!(trace.result, GpuExecutionResult::Failed(_)));
    assert!(matches!(
        trace.validation_state,
        ValidationState::Rejected(_)
    ));
    assert_eq!(
        trace.fallback_reason,
        Some(FallbackReason::CpuValidationFailed)
    );
    assert_eq!(
        trace.budget_evidence,
        Some(expected_stats_budget(&input, budget))
    );
}

#[test]
fn analytics_device_unavailable_fallback_trace_carries_budget_evidence() {
    let budget = GpuBudget::new(10_000_000, 10_000, 10_000_000).unwrap();
    let context = GpuAnalyticsBindingContext {
        job: GpuJobClass::Analytics,
        kill_switch: KillSwitchHandle::new(),
        budget,
        cpu_fallback: AnalyticsCpu,
        gpu_profile: GpuProfile::disabled(),
    };
    let input = AnalyticsInput {
        segment_id: 5,
        column_ids: vec![1, 2],
    };
    let permit = AnalyticsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    let (_, trace) = context
        .execute(&permit, &input, |_| {
            panic!("GPU closure must not run when permit is CPU-only")
        })
        .unwrap();

    assert_eq!(trace.result, GpuExecutionResult::Success);
    assert_eq!(trace.validation_state, ValidationState::NotValidated);
    assert_eq!(
        trace.fallback_reason,
        Some(FallbackReason::DeviceUnavailable)
    );
    assert_eq!(
        trace.budget_evidence,
        Some(expected_analytics_budget(&input, budget))
    );
}

#[test]
fn analytics_success_trace_carries_budget_evidence() {
    let budget = GpuBudget::new(10_000_000, 10_000, 10_000_000).unwrap();
    let context = GpuAnalyticsBindingContext {
        job: GpuJobClass::Analytics,
        kill_switch: KillSwitchHandle::new(),
        budget,
        cpu_fallback: AnalyticsCpu,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };
    let input = AnalyticsInput {
        segment_id: 6,
        column_ids: vec![4, 5],
    };
    let permit = AnalyticsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    let (_, trace) = context
        .execute(&permit, &input, |_| {
            Ok(AnalyticsOutput {
                row_count: 1_000,
                digest: 42,
            })
        })
        .unwrap();

    assert_eq!(trace.result, GpuExecutionResult::Success);
    assert_eq!(trace.validation_state, ValidationState::Validated);
    assert_eq!(trace.fallback_reason, None);
    assert_eq!(
        trace.budget_evidence,
        Some(expected_analytics_budget(&input, budget))
    );
}

#[test]
fn analytics_gpu_compute_failure_fallback_trace_carries_budget_evidence() {
    let budget = GpuBudget::new(10_000_000, 10_000, 10_000_000).unwrap();
    let context = GpuAnalyticsBindingContext {
        job: GpuJobClass::Analytics,
        kill_switch: KillSwitchHandle::new(),
        budget,
        cpu_fallback: AnalyticsCpu,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };
    let input = AnalyticsInput {
        segment_id: 7,
        column_ids: vec![1, 2, 3],
    };
    let permit = AnalyticsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    let (_, trace) = context
        .execute(&permit, &input, |_| {
            Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "simulated GPU failure",
            ))
        })
        .unwrap();

    assert!(matches!(trace.result, GpuExecutionResult::Failed(_)));
    assert_eq!(trace.validation_state, ValidationState::NotValidated);
    assert_eq!(
        trace.fallback_reason,
        Some(FallbackReason::GpuComputationFailed)
    );
    assert_eq!(
        trace.budget_evidence,
        Some(expected_analytics_budget(&input, budget))
    );
}

#[test]
fn kill_switch_cancellation_trace_contract_is_typed() {
    let trace = GpuExecutionTrace::cancelled_by_kill_switch(GpuJobClass::Analytics, 1, 2, 3);
    assert_eq!(trace.result, GpuExecutionResult::Cancelled);
    assert_eq!(
        trace.fallback_reason,
        Some(FallbackReason::KillSwitchActive)
    );
    assert_eq!(trace.validation_state, ValidationState::NotValidated);
}
