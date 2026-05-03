use andromeda_catalog::{CompatibilityPolicy, QualifiedName, TransactionPolicy};
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    ColumnDescriptor, ProcedureId,
};

use crate::Cardinality;

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
                for predicate in predicates {
                    predicate.validate()?;
                }
            }
            Self::Assert {
                predicate,
                failure_code,
            } => {
                predicate.validate()?;
                validate_symbol(failure_code, "SRPL assertion failure code")?;
            }
            Self::Update {
                target,
                predicates,
                assignments,
            } => {
                validate_qualified_name(target, "SRPL update target")?;
                if assignments.is_empty() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Srpl,
                        "SRPL update operation must declare at least one assignment",
                    ));
                }
                for predicate in predicates {
                    predicate.validate()?;
                }
                for assignment in assignments {
                    assignment.validate()?;
                }
            }
            Self::Emit { stream, values } => {
                validate_symbol(stream, "SRPL emit stream")?;
                if values.is_empty() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Srpl,
                        "SRPL emit operation must declare at least one value",
                    ));
                }
                for value in values {
                    value.validate()?;
                }
            }
            Self::Raise { code } => {
                validate_symbol(code, "SRPL raise code")?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SrplPredicateIr {
    InputEqualsField {
        input: String,
        binding: String,
        field: String,
    },
    FieldGreaterThanOrEqualInput {
        binding: String,
        field: String,
        input: String,
    },
}

impl SrplPredicateIr {
    fn validate(&self) -> AndromedaResult<()> {
        match self {
            Self::InputEqualsField {
                input,
                binding,
                field,
            }
            | Self::FieldGreaterThanOrEqualInput {
                binding,
                field,
                input,
            } => {
                validate_symbol(input, "SRPL predicate input")?;
                validate_symbol(binding, "SRPL predicate binding")?;
                validate_symbol(field, "SRPL predicate field")?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplAssignmentIr {
    pub field: String,
    pub value: SrplValueIr,
}

impl SrplAssignmentIr {
    fn validate(&self) -> AndromedaResult<()> {
        validate_symbol(&self.field, "SRPL assignment field")?;
        self.value.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplEmitValueIr {
    pub column: String,
    pub value: SrplValueIr,
}

impl SrplEmitValueIr {
    fn validate(&self) -> AndromedaResult<()> {
        validate_symbol(&self.column, "SRPL emit column")?;
        self.value.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SrplValueIr {
    Input(String),
    Field {
        binding: String,
        field: String,
    },
    Bool(bool),
    SubtractInput {
        binding: String,
        field: String,
        input: String,
    },
}

impl SrplValueIr {
    fn validate(&self) -> AndromedaResult<()> {
        match self {
            Self::Input(input) => validate_symbol(input, "SRPL value input")?,
            Self::Field { binding, field } => {
                validate_symbol(binding, "SRPL value binding")?;
                validate_symbol(field, "SRPL value field")?;
            }
            Self::Bool(_) => {}
            Self::SubtractInput {
                binding,
                field,
                input,
            } => {
                validate_symbol(binding, "SRPL subtract binding")?;
                validate_symbol(field, "SRPL subtract field")?;
                validate_symbol(input, "SRPL subtract input")?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplProcedureContractMetadata {
    pub object_id: CatalogObjectId,
    pub procedure_id: ProcedureId,
    pub catalog_version: CatalogVersion,
    pub structured_inputs: Vec<QualifiedName>,
    pub required_permissions: Vec<String>,
    pub transaction_policy: TransactionPolicy,
    pub compatibility_policy: CompatibilityPolicy,
}

fn validate_qualified_name(name: &QualifiedName, context: &str) -> AndromedaResult<()> {
    if name.parts().is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            format!("{context} must not be empty"),
        ));
    }

    Ok(())
}

fn validate_symbol(value: &str, context: &str) -> AndromedaResult<()> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            format!("{context} must not be empty"),
        ));
    };

    if !(first.is_ascii_alphabetic() || first == '_')
        || chars.any(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
    {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            format!("{context} must be an ASCII identifier"),
        ));
    }

    Ok(())
}
