//! GPU resource budget constraints and request validation.
//!
//! [`GpuBudget`] defines the maximum allowable resource consumption for a GPU
//! job. Callers construct a [`GpuBudgetRequest`] describing their estimated
//! resource needs and call [`GpuBudget::fits`] before dispatching GPU work.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Resource budget for a GPU job.
///
/// All fields are strictly positive; the constructor validates this invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuBudget {
    memory_bytes: u64,
    time_ms: u64,
    transfer_bytes: u64,
}

impl GpuBudget {
    /// Creates a new budget with the given constraints.
    ///
    /// # Errors
    ///
    /// Returns [`AndromedaError`] with kind [`AndromedaErrorKind::Resource`] if
    /// any of the three limits is zero.
    #[must_use = "the constructed GpuBudget must be used"]
    pub fn new(memory_bytes: u64, time_ms: u64, transfer_bytes: u64) -> AndromedaResult<Self> {
        if memory_bytes == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "GpuBudget memory_bytes must be greater than zero",
            ));
        }
        if time_ms == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "GpuBudget time_ms must be greater than zero",
            ));
        }
        if transfer_bytes == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "GpuBudget transfer_bytes must be greater than zero",
            ));
        }
        Ok(Self {
            memory_bytes,
            time_ms,
            transfer_bytes,
        })
    }

    /// Returns the memory limit in bytes.
    #[must_use]
    pub const fn memory_bytes(&self) -> u64 {
        self.memory_bytes
    }

    /// Returns the execution time limit in milliseconds.
    #[must_use]
    pub const fn time_ms(&self) -> u64 {
        self.time_ms
    }

    /// Returns the data transfer limit in bytes.
    #[must_use]
    pub const fn transfer_bytes(&self) -> u64 {
        self.transfer_bytes
    }

    /// Checks whether a request fits within this budget.
    ///
    /// # Errors
    ///
    /// Returns [`AndromedaError`] with kind [`AndromedaErrorKind::Resource`] if
    /// the request exceeds any single budget dimension.
    #[must_use = "budget fit result must be checked before dispatching GPU work"]
    pub fn fits(&self, requested: &GpuBudgetRequest) -> AndromedaResult<()> {
        if requested.memory_bytes > self.memory_bytes {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                format!(
                    "GPU memory request {} bytes exceeds budget {} bytes",
                    requested.memory_bytes, self.memory_bytes
                ),
            ));
        }
        if requested.time_ms > self.time_ms {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                format!(
                    "GPU time request {} ms exceeds budget {} ms",
                    requested.time_ms, self.time_ms
                ),
            ));
        }
        if requested.transfer_bytes > self.transfer_bytes {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                format!(
                    "GPU transfer request {} bytes exceeds budget {} bytes",
                    requested.transfer_bytes, self.transfer_bytes
                ),
            ));
        }
        Ok(())
    }
}

/// Requested resource consumption for a single GPU job.
///
/// Used as an argument to [`GpuBudget::fits`] to verify that the planned job
/// fits within the configured limits before dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuBudgetRequest {
    /// Estimated memory consumption in bytes.
    pub memory_bytes: u64,
    /// Estimated execution time in milliseconds.
    pub time_ms: u64,
    /// Estimated host-device transfer in bytes.
    pub transfer_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_memory_bytes_is_rejected() {
        assert!(GpuBudget::new(0, 100, 512).is_err());
    }

    #[test]
    fn zero_time_ms_is_rejected() {
        assert!(GpuBudget::new(1024, 0, 512).is_err());
    }

    #[test]
    fn zero_transfer_bytes_is_rejected() {
        assert!(GpuBudget::new(1024, 100, 0).is_err());
    }

    #[test]
    fn valid_budget_construction_succeeds() {
        let budget = GpuBudget::new(1024, 100, 512).unwrap();
        assert_eq!(budget.memory_bytes(), 1024);
        assert_eq!(budget.time_ms(), 100);
        assert_eq!(budget.transfer_bytes(), 512);
    }

    #[test]
    fn fits_returns_ok_when_request_is_within_budget() {
        let budget = GpuBudget::new(1024, 100, 512).unwrap();
        let request = GpuBudgetRequest {
            memory_bytes: 512,
            time_ms: 50,
            transfer_bytes: 256,
        };
        assert!(budget.fits(&request).is_ok());
    }

    #[test]
    fn fits_returns_ok_at_exact_budget_limits() {
        let budget = GpuBudget::new(1024, 100, 512).unwrap();
        let request = GpuBudgetRequest {
            memory_bytes: 1024,
            time_ms: 100,
            transfer_bytes: 512,
        };
        assert!(budget.fits(&request).is_ok());
    }
}
