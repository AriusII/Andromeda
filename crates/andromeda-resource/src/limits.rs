use crate::{ResourceLimitError, ResourceLimitField, ResourceResult};
use andromeda_hardware::ResourceBudget as HardwareResourceBudget;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ByteBudget(u64);

impl ByteBudget {
    pub const fn new(bytes: u64) -> Self {
        Self(bytes)
    }

    pub const fn bytes(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteLimit(u64);

impl ByteLimit {
    pub fn new(bytes: u64, field: ResourceLimitField) -> ResourceResult<Self> {
        if bytes == 0 {
            return Err(ResourceLimitError::ZeroLimit { field });
        }

        Ok(Self(bytes))
    }

    pub const fn bytes(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StreamLimit(u32);

impl StreamLimit {
    pub fn new(streams: u32) -> ResourceResult<Self> {
        if streams == 0 {
            return Err(ResourceLimitError::ZeroLimit {
                field: ResourceLimitField::StreamCount,
            });
        }

        Ok(Self(streams))
    }

    pub const fn streams(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceBudget {
    pub max_memory_bytes: ByteBudget,
    pub max_temp_bytes: ByteBudget,
    pub max_streams: StreamBudget,
}

impl ResourceBudget {
    pub const fn new(max_memory_bytes: u64, max_temp_bytes: u64, max_streams: u32) -> Self {
        Self {
            max_memory_bytes: ByteBudget::new(max_memory_bytes),
            max_temp_bytes: ByteBudget::new(max_temp_bytes),
            max_streams: StreamBudget::new(max_streams),
        }
    }

    pub fn total_bytes(self) -> ResourceResult<u64> {
        self.max_memory_bytes
            .bytes()
            .checked_add(self.max_temp_bytes.bytes())
            .ok_or(ResourceLimitError::ByteBudgetOverflow)
    }
}

impl From<HardwareResourceBudget> for ResourceBudget {
    fn from(budget: HardwareResourceBudget) -> Self {
        Self::new(
            budget.max_memory_bytes,
            budget.max_temp_bytes,
            budget.max_streams,
        )
    }
}

impl PartialEq<HardwareResourceBudget> for ResourceBudget {
    fn eq(&self, other: &HardwareResourceBudget) -> bool {
        self.max_memory_bytes.bytes() == other.max_memory_bytes
            && self.max_temp_bytes.bytes() == other.max_temp_bytes
            && self.max_streams.streams() == other.max_streams
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct StreamBudget(u32);

impl StreamBudget {
    pub const fn new(streams: u32) -> Self {
        Self(streams)
    }

    pub const fn streams(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits {
    pub max_memory_bytes: ByteLimit,
    pub max_temp_bytes: ByteBudget,
    pub max_streams: StreamLimit,
}

impl ResourceLimits {
    pub fn new(
        max_memory_bytes: u64,
        max_temp_bytes: u64,
        max_streams: u32,
    ) -> ResourceResult<Self> {
        Ok(Self {
            max_memory_bytes: ByteLimit::new(max_memory_bytes, ResourceLimitField::MemoryBytes)?,
            max_temp_bytes: ByteBudget::new(max_temp_bytes),
            max_streams: StreamLimit::new(max_streams)?,
        })
    }

    pub fn admit_budget(self, budget: ResourceBudget) -> ResourceResult<()> {
        if budget.max_memory_bytes.bytes() > self.max_memory_bytes.bytes() {
            return Err(ResourceLimitError::BudgetExceedsLimit {
                field: ResourceLimitField::MemoryBytes,
                budget: budget.max_memory_bytes.bytes(),
                limit: self.max_memory_bytes.bytes(),
            });
        }

        if budget.max_temp_bytes.bytes() > self.max_temp_bytes.bytes() {
            return Err(ResourceLimitError::BudgetExceedsLimit {
                field: ResourceLimitField::TempBytes,
                budget: budget.max_temp_bytes.bytes(),
                limit: self.max_temp_bytes.bytes(),
            });
        }

        if budget.max_streams.streams() > self.max_streams.streams() {
            return Err(ResourceLimitError::BudgetExceedsLimit {
                field: ResourceLimitField::StreamCount,
                budget: u64::from(budget.max_streams.streams()),
                limit: u64::from(self.max_streams.streams()),
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_limits_reject_zero_required_limits() {
        assert_eq!(
            ResourceLimits::new(0, 128, 1).unwrap_err(),
            ResourceLimitError::ZeroLimit {
                field: ResourceLimitField::MemoryBytes
            }
        );
        assert_eq!(
            ResourceLimits::new(128, 0, 0).unwrap_err(),
            ResourceLimitError::ZeroLimit {
                field: ResourceLimitField::StreamCount
            }
        );
    }

    #[test]
    fn resource_budget_checks_total_overflow() {
        assert_eq!(
            ResourceBudget::new(u64::MAX, 1, 1)
                .total_bytes()
                .unwrap_err(),
            ResourceLimitError::ByteBudgetOverflow
        );
    }

    #[test]
    fn resource_limits_admit_only_bounded_budgets() {
        let limits = ResourceLimits::new(256, 64, 8).unwrap();

        assert!(limits.admit_budget(ResourceBudget::new(128, 32, 8)).is_ok());
        assert_eq!(
            limits
                .admit_budget(ResourceBudget::new(128, 65, 8))
                .unwrap_err(),
            ResourceLimitError::BudgetExceedsLimit {
                field: ResourceLimitField::TempBytes,
                budget: 65,
                limit: 64
            }
        );
        assert_eq!(
            limits
                .admit_budget(ResourceBudget::new(128, 32, 9))
                .unwrap_err(),
            ResourceLimitError::BudgetExceedsLimit {
                field: ResourceLimitField::StreamCount,
                budget: 9,
                limit: 8
            }
        );
    }
}
