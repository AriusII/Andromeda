use andromeda_core::{
    AndromedaError, AndromedaErrorKind, CatalogVersion, ContractHash, ProcedureId,
};

use super::request::ProcedureResolveTarget;

/// Typed failures from the pre-transaction resolver boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcedureResolveError {
    InvalidRequest {
        message: String,
    },
    UnknownProcedure {
        target: ProcedureResolveTarget,
    },
    ContractMismatch {
        procedure_id: ProcedureId,
        expected: ContractHash,
        actual: ContractHash,
    },
    VersionMismatch {
        requested: CatalogVersion,
        actual: CatalogVersion,
    },
    InvalidResponse {
        message: String,
    },
    ResolverRejected {
        message: String,
    },
}

impl ProcedureResolveError {
    pub fn invalid_request(error: AndromedaError) -> Self {
        Self::InvalidRequest {
            message: error.message().to_string(),
        }
    }

    pub fn invalid_response(error: AndromedaError) -> Self {
        Self::InvalidResponse {
            message: error.message().to_string(),
        }
    }

    pub fn into_andromeda_error(self) -> AndromedaError {
        match self {
            Self::InvalidRequest { message } | Self::InvalidResponse { message } => {
                AndromedaError::new(AndromedaErrorKind::Contract, message)
            }
            Self::UnknownProcedure { .. } => AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "procedure resolver found no matching procedure",
            ),
            Self::ContractMismatch { .. } => AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure resolver contract hash mismatch",
            ),
            Self::VersionMismatch { .. } => AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "procedure resolver catalog version mismatch",
            ),
            Self::ResolverRejected { message } => {
                AndromedaError::new(AndromedaErrorKind::Catalog, message)
            }
        }
    }
}
