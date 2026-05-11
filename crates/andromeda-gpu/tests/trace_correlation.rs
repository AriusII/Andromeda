//! Verifies caller-supplied trace IDs on prototype execution paths.

use andromeda_error::AndromedaResult;
use andromeda_gpu::{
    budget::GpuBudget,
    fallback::CpuFallback,
    job::GpuJobClass,
    kill_switch::KillSwitchHandle,
    policy::GpuProfile,
    prototypes::{
        analytics::{AnalyticsInput, AnalyticsOutput, GpuAnalyticsBindingContext},
        permit::{AnalyticsExecutionPermit, StatsExecutionPermit},
        stats::{GpuStatsBindingContext, StatsInput},
    },
    validation::{GpuStatsValidationGate, Histogram},
};

struct StatsCpu;

impl CpuFallback for StatsCpu {
    type Input = StatsInput;
    type Output = Histogram;

    fn execute(&self, _input: &StatsInput) -> AndromedaResult<Histogram> {
        Ok(Histogram {
            buckets: vec![1, 2, 3],
        })
    }
}

struct AnalyticsCpu;

impl CpuFallback for AnalyticsCpu {
    type Input = AnalyticsInput;
    type Output = AnalyticsOutput;

    fn execute(&self, _input: &AnalyticsInput) -> AndromedaResult<AnalyticsOutput> {
        Ok(AnalyticsOutput {
            row_count: 10,
            digest: 20,
        })
    }
}

#[test]
fn stats_trace_id_is_non_zero_when_supplied() {
    let context = GpuStatsBindingContext {
        job: GpuJobClass::Statistics,
        kill_switch: KillSwitchHandle::new(),
        budget: GpuBudget::new(10_000_000, 10_000, 10_000_000).unwrap(),
        validation_gate: GpuStatsValidationGate::new(1.0).unwrap(),
        cpu_fallback: StatsCpu,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };
    let permit = StatsExecutionPermit::prevalidate(context.gpu_profile).unwrap();
    let input = StatsInput {
        snapshot_lsn: 1,
        column_id: 2,
    };

    let (_, trace) = context
        .execute_with_trace_id(101, &permit, &input, |_| {
            Ok(Histogram {
                buckets: vec![1, 2, 3],
            })
        })
        .unwrap();

    assert_eq!(trace.trace_id, 101);
}

#[test]
fn analytics_trace_id_is_non_zero_when_supplied() {
    let context = GpuAnalyticsBindingContext {
        job: GpuJobClass::Analytics,
        kill_switch: KillSwitchHandle::new(),
        budget: GpuBudget::new(10_000_000, 10_000, 10_000_000).unwrap(),
        cpu_fallback: AnalyticsCpu,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };
    let permit = AnalyticsExecutionPermit::prevalidate(context.gpu_profile).unwrap();
    let input = AnalyticsInput {
        segment_id: 1,
        column_ids: vec![2, 3],
    };

    let (_, trace) = context
        .execute_with_trace_id(202, &permit, &input, |_| {
            Ok(AnalyticsOutput {
                row_count: 10,
                digest: 20,
            })
        })
        .unwrap();

    assert_eq!(trace.trace_id, 202);
}
