use std::{error::Error, fmt};

pub type ResourceResult<T> = Result<T, ResourceLimitError>;
pub type ResourceScopeResult<T> = Result<T, ResourceScopeError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceLimitField {
    MemoryBytes,
    TempBytes,
    StreamCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceLimitError {
    ZeroLimit {
        field: ResourceLimitField,
    },
    ByteBudgetOverflow,
    BudgetExceedsLimit {
        field: ResourceLimitField,
        budget: u64,
        limit: u64,
    },
}

impl fmt::Display for ResourceLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit { field } => {
                write!(
                    f,
                    "resource limit field {field:?} must be greater than zero"
                )
            },
            Self::ByteBudgetOverflow => {
                f.write_str("resource byte budget overflows u64 when memory and temp are combined")
            },
            Self::BudgetExceedsLimit {
                field,
                budget,
                limit,
            } => write!(
                f,
                "resource budget field {field:?} value {budget} exceeds configured limit {limit}"
            ),
        }
    }
}

impl Error for ResourceLimitError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceScopeError {
    EmptyJobName,
}

impl fmt::Display for ResourceScopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyJobName => f.write_str("resource governance job name must not be empty"),
        }
    }
}

impl Error for ResourceScopeError {}
