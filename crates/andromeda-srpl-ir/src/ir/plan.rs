use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_procedure_contract::{CatalogObjectRef, ObjectKind, QualifiedName};

use crate::{Cardinality, identifier::validate_srpl_identifier as validate_symbol};

use super::{
    evidence::SrplCatalogBindingEvidence,
    procedure::MAX_SRPL_BODY_OPERATIONS,
    values::{
        SrplAssignmentIr, SrplEmitValueIr, SrplPredicateIr, validate_assignments,
        validate_emit_values, validate_predicates,
    },
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
            operation.validate()?;
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

    pub fn validate(&self) -> AndromedaResult<()> {
        match self {
            Self::ReadTable {
                source,
                binding,
                predicates,
                ..
            } => {
                source.validate_for_definition(ObjectKind::Table)?;
                validate_symbol(binding, "bound SRPL read binding")?;
                validate_predicates(predicates)?;
            },
            Self::Assert {
                predicate,
                failure_code,
                ..
            } => {
                predicate.validate()?;
                validate_symbol(failure_code, "bound SRPL assertion failure code")?;
            },
            Self::UpdateTable {
                target,
                predicates,
                assignments,
                affected_rows_exact,
                ..
            } => {
                target.validate_for_definition(ObjectKind::Table)?;
                if assignments.is_empty() {
                    return Err(srpl_error(
                        "bound SRPL update operation must declare at least one assignment",
                    ));
                }
                validate_predicates(predicates)?;
                validate_assignments(assignments)?;
                if matches!(affected_rows_exact, Some(0)) {
                    return Err(srpl_error(
                        "bound SRPL update affected rows must be greater than zero",
                    ));
                }
            },
            Self::Emit { stream, values, .. } => {
                validate_symbol(stream, "bound SRPL emit stream")?;
                if values.is_empty() {
                    return Err(srpl_error(
                        "bound SRPL emit operation must declare at least one value",
                    ));
                }
                validate_emit_values(values)?;
            },
            Self::Raise { code, .. } => {
                validate_symbol(code, "bound SRPL raise code")?;
            },
        }

        Ok(())
    }
}

fn srpl_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Srpl, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_procedure_contract::{ProcedureContractRef, QualifiedName};
    use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash, ProcedureId};

    use crate::{
        ArithOp, ConstantLiteral, MAX_EXPR_DEPTH, SrplValueIr,
        ir::evidence::SrplCatalogBindingEvidence,
    };

    fn object(name: &str, kind: ObjectKind) -> CatalogObjectRef {
        CatalogObjectRef {
            object_id: CatalogObjectId::new(7),
            name: QualifiedName::parse(name).unwrap(),
            kind,
            catalog_version: CatalogVersion::new(11),
        }
    }

    fn table_ref() -> CatalogObjectRef {
        object("Inventory.ProductStock", ObjectKind::Table)
    }

    fn procedure_ref() -> CatalogObjectRef {
        object("Inventory.ReserveStock", ObjectKind::Procedure)
    }

    fn valid_predicate() -> SrplPredicateIr {
        SrplPredicateIr::InputEqualsField {
            input: "ProductId".to_string(),
            binding: "stock".to_string(),
            field: "ProductId".to_string(),
        }
    }

    fn valid_assignment() -> SrplAssignmentIr {
        SrplAssignmentIr {
            field: "QuantityAvailable".to_string(),
            value: SrplValueIr::Input("Quantity".to_string()),
        }
    }

    fn valid_emit_value() -> SrplEmitValueIr {
        SrplEmitValueIr {
            column: "Reserved".to_string(),
            value: SrplValueIr::Constant(ConstantLiteral::Bool(true)),
        }
    }

    fn valid_body() -> BoundSrplBodyPlan {
        BoundSrplBodyPlan {
            operations: vec![BoundSrplOperationPlan::Emit {
                ordinal: 0,
                stream: "Reservation".to_string(),
                values: vec![valid_emit_value()],
            }],
        }
    }

    fn error_for(operation: BoundSrplOperationPlan) -> AndromedaError {
        BoundSrplBodyPlan {
            operations: vec![operation],
        }
        .validate()
        .unwrap_err()
    }

    #[test]
    fn body_plan_rejects_bound_operation_empty_identifiers() {
        let cases = [
            BoundSrplOperationPlan::ReadTable {
                ordinal: 0,
                source: table_ref(),
                binding: String::new(),
                cardinality: Cardinality::One,
                predicates: vec![],
            },
            BoundSrplOperationPlan::Assert {
                ordinal: 0,
                predicate: valid_predicate(),
                failure_code: String::new(),
            },
            BoundSrplOperationPlan::UpdateTable {
                ordinal: 0,
                target: table_ref(),
                predicates: vec![],
                assignments: vec![SrplAssignmentIr {
                    field: String::new(),
                    value: SrplValueIr::Input("Quantity".to_string()),
                }],
                affected_rows_exact: Some(1),
            },
            BoundSrplOperationPlan::Emit {
                ordinal: 0,
                stream: String::new(),
                values: vec![valid_emit_value()],
            },
            BoundSrplOperationPlan::Raise {
                ordinal: 0,
                code: String::new(),
            },
        ];

        for operation in cases {
            let error = error_for(operation);

            assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
            assert!(error.message().contains("must not be empty"));
        }
    }

    #[test]
    fn body_plan_rejects_emit_without_values() {
        let error = error_for(BoundSrplOperationPlan::Emit {
            ordinal: 0,
            stream: "Reservation".to_string(),
            values: vec![],
        });

        assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
        assert!(error.message().contains("at least one value"));
    }

    #[test]
    fn body_plan_rejects_update_without_assignments() {
        let error = error_for(BoundSrplOperationPlan::UpdateTable {
            ordinal: 0,
            target: table_ref(),
            predicates: vec![valid_predicate()],
            assignments: vec![],
            affected_rows_exact: Some(1),
        });

        assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
        assert!(error.message().contains("at least one assignment"));
    }

    #[test]
    fn body_plan_rejects_update_with_zero_exact_affected_rows() {
        let error = error_for(BoundSrplOperationPlan::UpdateTable {
            ordinal: 0,
            target: table_ref(),
            predicates: vec![valid_predicate()],
            assignments: vec![valid_assignment()],
            affected_rows_exact: Some(0),
        });

        assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
        assert!(error.message().contains("greater than zero"));
    }

    #[test]
    fn body_plan_rejects_values_deeper_than_local_expression_bound() {
        let mut value = SrplValueIr::Input("Quantity".to_string());
        for _ in 0..=MAX_EXPR_DEPTH {
            value = SrplValueIr::BinaryArith {
                op: ArithOp::Add,
                left: Box::new(value),
                right: Box::new(SrplValueIr::Constant(ConstantLiteral::Int64(1))),
            };
        }

        let error = error_for(BoundSrplOperationPlan::Emit {
            ordinal: 0,
            stream: "Reservation".to_string(),
            values: vec![SrplEmitValueIr {
                column: "Remaining".to_string(),
                value,
            }],
        });

        assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
        assert!(error.message().contains("maximum nesting depth"));
    }

    #[test]
    fn executable_plan_rejects_invalid_binding_evidence() {
        let procedure_object = procedure_ref();
        let plan = ExecutableProcedurePlan {
            procedure_name: procedure_object.name.clone(),
            body: valid_body(),
            evidence: SrplCatalogBindingEvidence {
                catalog_version: procedure_object.catalog_version,
                procedure_object,
                procedure_contract: ProcedureContractRef {
                    procedure_id: ProcedureId::new(42),
                    contract_hash: ContractHash::zero(),
                    catalog_version: CatalogVersion::new(11),
                },
                bound_objects: vec![],
            },
        };

        let error = plan.validate().unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
        assert!(error.message().contains("hash"));
    }
}
