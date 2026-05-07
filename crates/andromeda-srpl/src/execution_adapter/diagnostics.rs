use andromeda_error::{AndromedaError, AndromedaErrorKind};

use crate::procedure_model::Cardinality;

use super::contracts::SrplRowBound;

/// Adapter-visible typed failure surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SrplExecutionFailure {
    SemanticViolation(String),
    CatalogBindingViolation(String),
    ContractViolation(String),
    CardinalityViolation {
        expected: Cardinality,
        bound: SrplRowBound,
        actual_rows: u64,
    },
    ResourceLimitExceeded(String),
    AdapterRejected(String),
}

impl SrplExecutionFailure {
    pub fn into_andromeda_error(self) -> AndromedaError {
        match self {
            Self::SemanticViolation(message) => {
                AndromedaError::new(AndromedaErrorKind::Srpl, message)
            }
            Self::CatalogBindingViolation(message) => {
                AndromedaError::new(AndromedaErrorKind::Catalog, message)
            }
            Self::ContractViolation(message) => {
                AndromedaError::new(AndromedaErrorKind::Contract, message)
            }
            Self::CardinalityViolation { .. } => AndromedaError::new(
                AndromedaErrorKind::Execution,
                "SRPL adapter operation violated its cardinality or row bound",
            ),
            Self::ResourceLimitExceeded(message) => {
                AndromedaError::new(AndromedaErrorKind::Resource, message)
            }
            Self::AdapterRejected(message) => {
                AndromedaError::new(AndromedaErrorKind::Execution, message)
            }
        }
    }
}

impl From<AndromedaError> for SrplExecutionFailure {
    fn from(error: AndromedaError) -> Self {
        let message = error.message().to_string();
        match error.kind() {
            AndromedaErrorKind::Catalog => Self::CatalogBindingViolation(message),
            AndromedaErrorKind::Contract => Self::ContractViolation(message),
            AndromedaErrorKind::Resource => Self::ResourceLimitExceeded(message),
            AndromedaErrorKind::Srpl => Self::SemanticViolation(message),
            _ => Self::AdapterRejected(message),
        }
    }
}
