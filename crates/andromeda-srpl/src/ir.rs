use andromeda_catalog::{
    CatalogObjectRef, CompatibilityPolicy, MultiResultPolicy, ProcedureContractRef,
    ProcedureErrorPolicy, ProtocolLayoutRef, QualifiedName, ResultMetadataPolicy, StatsVersion,
    TransactionPolicy,
};
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    ColumnDescriptor, ContractHash, ProcedureId,
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
                affected_rows_exact,
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
                if matches!(affected_rows_exact, Some(0)) {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Srpl,
                        "SRPL update affected rows must be greater than zero",
                    ));
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplCatalogBindingEvidence {
    pub catalog_version: CatalogVersion,
    pub procedure_object: CatalogObjectRef,
    pub procedure_contract: ProcedureContractRef,
    pub stock_object: SrplObjectBindingEvidence,
    pub reservation_object: SrplObjectBindingEvidence,
}

impl SrplCatalogBindingEvidence {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.procedure_object
            .validate_for_definition(andromeda_catalog::ObjectKind::Procedure)?;
        self.procedure_contract.validate()?;
        self.stock_object
            .validate(andromeda_catalog::ObjectKind::Table)?;
        self.reservation_object
            .validate(andromeda_catalog::ObjectKind::StructuredObject)?;

        if self.procedure_object.catalog_version != self.catalog_version
            || self.procedure_contract.catalog_version != self.catalog_version
            || self.stock_object.object.catalog_version != self.catalog_version
            || self.reservation_object.object.catalog_version != self.catalog_version
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "SRPL executable plan evidence requires exact catalog version match",
            ));
        }

        if self.procedure_contract.contract_hash.is_zero()
            || self.stock_object.shape_hash.is_zero()
            || self.reservation_object.shape_hash.is_zero()
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "SRPL executable plan binding hashes must not be zero",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplObjectBindingEvidence {
    pub object: CatalogObjectRef,
    pub shape_hash: ContractHash,
}

impl SrplObjectBindingEvidence {
    pub fn validate(&self, expected_kind: andromeda_catalog::ObjectKind) -> AndromedaResult<()> {
        self.object.validate_for_definition(expected_kind)?;
        if self.shape_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "SRPL object binding shape hash must not be zero",
            ));
        }
        Ok(())
    }
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
