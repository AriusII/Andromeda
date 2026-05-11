//! Verifies that a GPU analytics output that diverges from the CPU shadow is
//! rejected and the CPU result is returned instead.

use andromeda_error::AndromedaResult;
use andromeda_gpu::{
    budget::GpuBudget,
    fallback::{CpuFallback, ValidationState},
    job::{FallbackReason, GpuJobClass},
    kill_switch::KillSwitchHandle,
    policy::GpuProfile,
    prototypes::{
        analytics::{AnalyticsInput, AnalyticsOutput, GpuAnalyticsBindingContext},
        permit::AnalyticsExecutionPermit,
    },
};

/// CPU shadow always returns a fixed authoritative analytics output.
struct AuthoritativeCpuAnalytics;

impl CpuFallback for AuthoritativeCpuAnalytics {
    type Input = AnalyticsInput;
    type Output = AnalyticsOutput;

    fn execute(&self, _input: &AnalyticsInput) -> AndromedaResult<AnalyticsOutput> {
        Ok(AnalyticsOutput {
            row_count: 1_000,
            digest: 0xDEAD_BEEF,
        })
    }
}

#[test]
fn divergent_gpu_analytics_output_is_rejected_and_cpu_result_is_returned() {
    let context = GpuAnalyticsBindingContext {
        job: GpuJobClass::Analytics,
        kill_switch: KillSwitchHandle::new(),
        budget: GpuBudget::new(1_024 * 1_024, 1_000, 1_024 * 1_024).unwrap(),
        cpu_fallback: AuthoritativeCpuAnalytics,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };

    let input = AnalyticsInput {
        segment_id: 1,
        column_ids: vec![0, 1, 2],
    };
    let permit = AnalyticsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    // GPU produces a wrong row_count and digest.
    let divergent_gpu_output = AnalyticsOutput {
        row_count: 999,     // differs from CPU's 1000
        digest: 0xBAD_F00D, // differs from CPU's 0xDEAD_BEEF
    };

    let (returned_output, trace) = context
        .execute(&permit, &input, |_| Ok(divergent_gpu_output.clone()))
        .unwrap();

    // The returned output must be the CPU authoritative result.
    let cpu_expected = AnalyticsOutput {
        row_count: 1_000,
        digest: 0xDEAD_BEEF,
    };
    assert_eq!(
        returned_output, cpu_expected,
        "divergent GPU analytics output must be replaced by CPU shadow"
    );

    // Trace must show validation was rejected.
    assert!(
        matches!(trace.validation_state, ValidationState::Rejected(_)),
        "validation_state must be Rejected, got {:?}",
        trace.validation_state
    );

    // Trace must record CpuValidationFailed.
    assert_eq!(
        trace.fallback_reason,
        Some(FallbackReason::CpuValidationFailed),
        "fallback_reason must be CpuValidationFailed"
    );

    // The divergent GPU output must not be published.
    assert_ne!(
        returned_output, divergent_gpu_output,
        "divergent GPU analytics output must not be published"
    );
}

#[test]
fn matching_gpu_analytics_output_is_validated_and_returned() {
    let context = GpuAnalyticsBindingContext {
        job: GpuJobClass::Analytics,
        kill_switch: KillSwitchHandle::new(),
        budget: GpuBudget::new(1_024 * 1_024, 1_000, 1_024 * 1_024).unwrap(),
        cpu_fallback: AuthoritativeCpuAnalytics,
        gpu_profile: GpuProfile::batch_analytics_only(),
    };

    let input = AnalyticsInput {
        segment_id: 1,
        column_ids: vec![0],
    };
    let permit = AnalyticsExecutionPermit::prevalidate(context.gpu_profile).unwrap();

    // GPU matches CPU exactly.
    let (returned_output, trace) = context
        .execute(&permit, &input, |_| {
            Ok(AnalyticsOutput {
                row_count: 1_000,
                digest: 0xDEAD_BEEF,
            })
        })
        .unwrap();

    assert_eq!(returned_output.row_count, 1_000);
    assert_eq!(returned_output.digest, 0xDEAD_BEEF);
    assert_eq!(
        trace.validation_state,
        ValidationState::Validated,
        "matching GPU output must be Validated"
    );
    assert_eq!(trace.fallback_reason, None);
}
