use andromeda_catalog::{CatalogObjectRef, QualifiedName};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::Cardinality;

use super::{
    evidence::SrplCatalogBindingEvidence,
    procedure::MAX_SRPL_BODY_OPERATIONS,
    values::{SrplAssignmentIr, SrplEmitValueIr, SrplPredicateIr},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableProcedurePlan {
    pub procedure_name: QualifiedName,
    pub body: BoundSrplBodyPlan,
    pub evidence: SrplCatalogBindingEvidence,
}

impl ExecutableProcedurePlan {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.evidence.validate()?;
        self.body.validate()?;
        if self.procedure_name != self.evidence.procedure_object.name {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "executable SRPL plan procedure name must match binding evidence",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundSrplBodyPlan {
    pub operations: Vec<BoundSrplOperationPlan>,
}

impl BoundSrplBodyPlan {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.operations.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "executable SRPL body plan must contain deterministic operations",
            ));
        }

        if self.operations.len() > MAX_SRPL_BODY_OPERATIONS {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "executable SRPL body plan exceeds the bounded operation limit",
            ));
        }

        for (expected_ordinal, operation) in self.operations.iter().enumerate() {
            if operation.ordinal() != expected_ordinal as u32 {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Srpl,
                    "executable SRPL body plan ordinals must be dense and zero-based",
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundSrplOperationPlan {
    ReadTable {
        ordinal: u32,
        source: CatalogObjectRef,
        binding: String,
        cardinality: Cardinality,
        predicates: Vec<SrplPredicateIr>,
    },
    Assert {
        ordinal: u32,
        predicate: SrplPredicateIr,
        failure_code: String,
    },
    UpdateTable {
        ordinal: u32,
        target: CatalogObjectRef,
        predicates: Vec<SrplPredicateIr>,
        assignments: Vec<SrplAssignmentIr>,
        affected_rows_exact: Option<u64>,
    },
    Emit {
        ordinal: u32,
        stream: String,
        values: Vec<SrplEmitValueIr>,
    },
    Raise {
        ordinal: u32,
        code: String,
    },
}

impl BoundSrplOperationPlan {
    pub fn ordinal(&self) -> u32 {
        match self {
            Self::ReadTable { ordinal, .. }
            | Self::Assert { ordinal, .. }
            | Self::UpdateTable { ordinal, .. }
            | Self::Emit { ordinal, .. }
            | Self::Raise { ordinal, .. } => *ordinal,
        }
    }
}
