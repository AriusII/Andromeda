//! GPU output validation gate for statistics workloads.
//!
//! [`GpuStatsValidationGate`] compares a GPU-produced [`Histogram`] against a
//! CPU shadow histogram using a bucket-wise relative deviation check. If any
//! bucket exceeds the configured threshold, the gate returns
//! [`ValidationState::Rejected`][crate::fallback::ValidationState::Rejected].
//!
//! Only a [`Validated`][crate::fallback::ValidationState::Validated] result
//! permits the GPU histogram to be published. All other outcomes require the
//! CPU shadow to be used instead.

#![allow(clippy::cast_precision_loss)]

use crate::fallback::ValidationState;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// A histogram represented as a sequence of bucket counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Histogram {
    /// Ordered bucket counts.
    pub buckets: Vec<u64>,
}

/// Validates a GPU-produced histogram against a CPU shadow using a bucket-wise
/// relative deviation threshold.
///
/// Construct with [`GpuStatsValidationGate::new`], then call
/// [`validate_histogram`][GpuStatsValidationGate::validate_histogram] with
/// both histograms after the GPU and CPU paths have completed.
#[derive(Debug, Clone)]
pub struct GpuStatsValidationGate {
    max_deviation_pct: f64,
}

impl GpuStatsValidationGate {
    /// Creates a validation gate with the given maximum relative deviation
    /// percentage.
    ///
    /// # Errors
    ///
    /// Returns [`AndromedaError`] with kind [`AndromedaErrorKind::Contract`] if
    /// `max_deviation_pct` is not in the range `0.0..=100.0`.
    #[must_use = "the constructed GpuStatsValidationGate must be used"]
    pub fn new(max_deviation_pct: f64) -> AndromedaResult<Self> {
        if !(0.0..=100.0).contains(&max_deviation_pct) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                format!(
                    "GpuStatsValidationGate max_deviation_pct must be in 0.0..=100.0, got {max_deviation_pct}"
                ),
            ));
        }
        Ok(Self { max_deviation_pct })
    }

    /// Returns the configured deviation threshold as a percentage.
    #[must_use]
    pub fn max_deviation_pct(&self) -> f64 {
        self.max_deviation_pct
    }

    /// Compares `gpu` against `cpu_shadow` bucket-by-bucket.
    ///
    /// - Histograms must have the same length; a length mismatch produces
    ///   [`Rejected`][ValidationState::Rejected].
    /// - If a CPU bucket is zero and the corresponding GPU bucket is also zero,
    ///   the bucket is treated as matching exactly.
    /// - If a CPU bucket is zero but the GPU bucket is non-zero, the deviation
    ///   is considered infinite and the bucket is rejected.
    /// - Otherwise, the relative deviation is
    ///   `|gpu - cpu| / cpu * 100.0` and must not exceed
    ///   [`max_deviation_pct`][Self::max_deviation_pct].
    ///
    /// Returns [`Validated`][ValidationState::Validated] only when every bucket
    /// passes.
    ///
    /// # Errors
    ///
    /// This function does not currently return an error; the `AndromedaResult`
    /// wrapper is reserved for future fault injection and observability hooks.
    #[must_use = "validation result must be checked before publishing GPU output"]
    pub fn validate_histogram(
        &self,
        gpu: &Histogram,
        cpu_shadow: &Histogram,
    ) -> AndromedaResult<ValidationState> {
        if gpu.buckets.len() != cpu_shadow.buckets.len() {
            return Ok(ValidationState::Rejected(format!(
                "histogram bucket count mismatch: GPU has {}, CPU has {}",
                gpu.buckets.len(),
                cpu_shadow.buckets.len()
            )));
        }

        for (index, (gpu_count, cpu_count)) in gpu
            .buckets
            .iter()
            .zip(cpu_shadow.buckets.iter())
            .enumerate()
        {
            let gpu_f = *gpu_count as f64;
            let cpu_f = *cpu_count as f64;

            if *cpu_count == 0 {
                if *gpu_count != 0 {
                    return Ok(ValidationState::Rejected(format!(
                        "bucket {index}: CPU count is zero but GPU count is {gpu_count}; \
                         relative deviation is infinite"
                    )));
                }
                // Both zero — exact match, continue.
                continue;
            }

            let deviation_pct = (gpu_f - cpu_f).abs() / cpu_f * 100.0;
            if deviation_pct > self.max_deviation_pct {
                return Ok(ValidationState::Rejected(format!(
                    "bucket {index}: relative deviation {deviation_pct:.4}% exceeds \
                     threshold {:.4}% (GPU={gpu_count}, CPU={cpu_count})",
                    self.max_deviation_pct
                )));
            }
        }

        Ok(ValidationState::Validated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_rejects_deviation_above_threshold() {
        let gate = GpuStatsValidationGate::new(1.0).unwrap();
        // 3% deviation in bucket 0 exceeds 1% threshold
        let gpu = Histogram {
            buckets: vec![103, 200],
        };
        let cpu = Histogram {
            buckets: vec![100, 200],
        };
        let state = gate.validate_histogram(&gpu, &cpu).unwrap();
        assert!(matches!(state, ValidationState::Rejected(_)));
    }

    #[test]
    fn gate_accepts_deviation_within_threshold() {
        let gate = GpuStatsValidationGate::new(5.0).unwrap();
        // 3% deviation — within the 5% threshold
        let gpu = Histogram {
            buckets: vec![103, 200],
        };
        let cpu = Histogram {
            buckets: vec![100, 200],
        };
        assert_eq!(
            gate.validate_histogram(&gpu, &cpu).unwrap(),
            ValidationState::Validated
        );
    }

    #[test]
    fn gate_rejects_length_mismatch() {
        let gate = GpuStatsValidationGate::new(5.0).unwrap();
        let gpu = Histogram { buckets: vec![1] };
        let cpu = Histogram {
            buckets: vec![1, 2],
        };
        assert!(matches!(
            gate.validate_histogram(&gpu, &cpu).unwrap(),
            ValidationState::Rejected(_)
        ));
    }

    #[test]
    fn gate_rejects_nonzero_gpu_against_zero_cpu() {
        let gate = GpuStatsValidationGate::new(100.0).unwrap();
        let gpu = Histogram { buckets: vec![1] };
        let cpu = Histogram { buckets: vec![0] };
        assert!(matches!(
            gate.validate_histogram(&gpu, &cpu).unwrap(),
            ValidationState::Rejected(_)
        ));
    }

    #[test]
    fn gate_accepts_both_zero_buckets() {
        let gate = GpuStatsValidationGate::new(0.0).unwrap();
        let hist = Histogram {
            buckets: vec![0, 0],
        };
        assert_eq!(
            gate.validate_histogram(&hist, &hist).unwrap(),
            ValidationState::Validated
        );
    }

    #[test]
    fn constructor_rejects_out_of_range_threshold() {
        assert!(GpuStatsValidationGate::new(-0.1).is_err());
        assert!(GpuStatsValidationGate::new(100.1).is_err());
    }

    #[test]
    fn constructor_accepts_boundary_values() {
        assert!(GpuStatsValidationGate::new(0.0).is_ok());
        assert!(GpuStatsValidationGate::new(100.0).is_ok());
    }
}
