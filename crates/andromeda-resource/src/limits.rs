use crate::{ResourceLimitError, ResourceLimitField, ResourceResult};

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
    pub memory_bytes: ByteBudget,
    pub temp_bytes: ByteBudget,
}

impl ResourceBudget {
    pub fn new(memory_bytes: u64, temp_bytes: u64) -> ResourceResult<Self> {
        let budget = Self {
            memory_bytes: ByteBudget::new(memory_bytes),
            temp_bytes: ByteBudget::new(temp_bytes),
        };
        budget.total_bytes()?;
        Ok(budget)
    }

    pub fn total_bytes(self) -> ResourceResult<u64> {
        self.memory_bytes
            .bytes()
            .checked_add(self.temp_bytes.bytes())
            .ok_or(ResourceLimitError::ByteBudgetOverflow)
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
        if budget.memory_bytes.bytes() > self.max_memory_bytes.bytes() {
            return Err(ResourceLimitError::BudgetExceedsLimit {
                field: ResourceLimitField::MemoryBytes,
                budget: budget.memory_bytes.bytes(),
                limit: self.max_memory_bytes.bytes(),
            });
        }

        if budget.temp_bytes.bytes() > self.max_temp_bytes.bytes() {
            return Err(ResourceLimitError::BudgetExceedsLimit {
                field: ResourceLimitField::TempBytes,
                budget: budget.temp_bytes.bytes(),
                limit: self.max_temp_bytes.bytes(),
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
            ResourceBudget::new(u64::MAX, 1).unwrap_err(),
            ResourceLimitError::ByteBudgetOverflow
        );
    }

    #[test]
    fn resource_limits_admit_only_bounded_budgets() {
        let limits = ResourceLimits::new(256, 64, 8).unwrap();

        assert!(
            limits
                .admit_budget(ResourceBudget::new(128, 32).unwrap())
                .is_ok()
        );
        assert_eq!(
            limits
                .admit_budget(ResourceBudget::new(128, 65).unwrap())
                .unwrap_err(),
            ResourceLimitError::BudgetExceedsLimit {
                field: ResourceLimitField::TempBytes,
                budget: 65,
                limit: 64
            }
        );
    }
}
