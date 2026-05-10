use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_procedure_contract::{
    CompatibilityPolicy, MultiResultPolicy, ProcedureErrorPolicy, ProtocolLayoutRef, QualifiedName,
    ResultMetadataPolicy, StatsVersion, TransactionPolicy,
};
use andromeda_types::{CatalogObjectId, CatalogVersion, ColumnDescriptor, ProcedureId};

use crate::{Cardinality, identifier::validate_srpl_identifier as validate_symbol};

use super::{
    validation::validate_qualified_name,
    values::{
        SrplAssignmentIr, SrplEmitValueIr, SrplPredicateIr, validate_assignments,
        validate_emit_values, validate_predicates,
    },
};

pub const MAX_SRPL_BODY_OPERATIONS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplProcedureIr {
    pub name: QualifiedName,
    pub inputs: Vec<ColumnDescriptor>,
    pub result_streams: Vec<SrplResultStreamIr>,
    pub body: SrplProcedureBodyIr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplResultStreamIr {
    pub name: String,
    pub cardinality: Cardinality,
    pub columns: Vec<ColumnDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplProcedureBodyIr {
    pub operations: Vec<SrplBusinessOperationIr>,
}

impl SrplProcedureBodyIr {
    pub fn empty() -> Self {
        Self {
            operations: Vec::new(),
        }
    }

    pub fn validate_bounded(&self) -> AndromedaResult<()> {
        if self.operations.len() > MAX_SRPL_BODY_OPERATIONS {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL procedure body exceeds the bounded operation limit",
            ));
        }

        for (expected_ordinal, operation) in self.operations.iter().enumerate() {
            if operation.ordinal != expected_ordinal as u32 {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Srpl,
                    "SRPL body operation ordinals must be dense and zero-based",
                ));
            }
            operation.kind.validate()?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplBusinessOperationIr {
    pub ordinal: u32,
    pub kind: SrplBusinessOperationKindIr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SrplBusinessOperationKindIr {
    Read {
        source: QualifiedName,
        binding: String,
        cardinality: Cardinality,
        predicates: Vec<SrplPredicateIr>,
    },
    Assert {
        predicate: SrplPredicateIr,
        failure_code: String,
    },
    Update {
        target: QualifiedName,
        predicates: Vec<SrplPredicateIr>,
        assignments: Vec<SrplAssignmentIr>,
        affected_rows_exact: Option<u64>,
    },
    Emit {
        stream: String,
        values: Vec<SrplEmitValueIr>,
    },
    Raise {
        code: String,
    },
}

impl SrplBusinessOperationKindIr {
    fn validate(&self) -> AndromedaResult<()> {
        match self {
            Self::Read {
                source,
                binding,
                predicates,
                ..
            } => {
                validate_qualified_name(source, "SRPL read source")?;
                validate_symbol(binding, "SRPL read binding")?;
                validate_predicates(predicates)?;
            },
            Self::Assert {
                predicate,
                failure_code,
            } => {
                predicate.validate()?;
                validate_symbol(failure_code, "SRPL assertion failure code")?;
            },
            Self::Update {
                target,
                predicates,
                assignments,
                affected_rows_exact,
            } => {
                validate_qualified_name(target, "SRPL update target")?;
                if assignments.is_empty() {
                    return Err(srpl_error(
                        "SRPL update operation must declare at least one assignment",
                    ));
                }
                validate_predicates(predicates)?;
                validate_assignments(assignments)?;
                if matches!(affected_rows_exact, Some(0)) {
                    return Err(srpl_error(
                        "SRPL update affected rows must be greater than zero",
                    ));
                }
            },
            Self::Emit { stream, values } => {
                validate_symbol(stream, "SRPL emit stream")?;
                if values.is_empty() {
                    return Err(srpl_error(
                        "SRPL emit operation must declare at least one value",
                    ));
                }
                validate_emit_values(values)?;
            },
            Self::Raise { code } => {
                validate_symbol(code, "SRPL raise code")?;
            },
        }

        Ok(())
    }
}

fn srpl_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Srpl, message)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplProcedureContractMetadata {
    pub object_id: CatalogObjectId,
    pub procedure_id: ProcedureId,
    pub catalog_version: CatalogVersion,
    pub stats_version: StatsVersion,
    pub protocol_layout: ProtocolLayoutRef,
    pub structured_inputs: Vec<QualifiedName>,
    pub required_permissions: Vec<String>,
    pub transaction_policy: TransactionPolicy,
    pub compatibility_policy: CompatibilityPolicy,
    pub result_metadata_policy: ResultMetadataPolicy,
    pub error_policy: ProcedureErrorPolicy,
    pub multi_result_policy: MultiResultPolicy,
}
