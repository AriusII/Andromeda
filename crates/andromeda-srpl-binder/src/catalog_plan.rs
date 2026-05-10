//! Executable SRPL catalog binding.

use std::collections::{BTreeMap, BTreeSet};

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_procedure_contract::{
    CatalogObjectRef, ObjectKind, ProcedureContract, QualifiedName, ResultStreamCardinality,
};
use andromeda_srpl_ir::{
    BoundSrplBodyPlan, BoundSrplOperationPlan, Cardinality, ExecutableProcedurePlan,
    SrplAssignmentIr, SrplBusinessOperationIr, SrplBusinessOperationKindIr,
    SrplCatalogBindingEvidence, SrplEmitValueIr, SrplObjectBindingEvidence, SrplPredicateIr,
    SrplProcedureBodyIr, SrplProcedureIr, SrplValueIr, validate_no_sql_like_symbols,
};
use andromeda_types::{CatalogVersion, ColumnDescriptor, ContractHash};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplCatalogTableBinding {
    pub object: CatalogObjectRef,
    pub columns: Vec<ColumnDescriptor>,
    pub shape_hash: ContractHash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplCatalogStructuredObjectBinding {
    pub object: CatalogObjectRef,
    pub fields: Vec<ColumnDescriptor>,
    pub shape_hash: ContractHash,
}

pub trait SrplExecutableCatalogView {
    fn catalog_version(&self) -> CatalogVersion;

    fn lookup_procedure_contract(&self, name: &QualifiedName)
    -> AndromedaResult<ProcedureContract>;

    fn lookup_table(&self, name: &QualifiedName) -> AndromedaResult<SrplCatalogTableBinding>;

    fn try_lookup_structured_object(
        &self,
        name: &QualifiedName,
    ) -> AndromedaResult<Option<SrplCatalogStructuredObjectBinding>>;
}

pub fn bind_executable_procedure_plan(
    ir: &SrplProcedureIr,
    catalog: &impl SrplExecutableCatalogView,
) -> AndromedaResult<ExecutableProcedurePlan> {
    ir.body.validate_bounded()?;
    validate_no_sql_like_symbols(ir)?;

    let procedure = catalog.lookup_procedure_contract(&ir.name)?;
    procedure.validate_canonical_hash()?;
    validate_signature_matches_contract(ir, &procedure)?;

    if procedure.object.catalog_version != catalog.catalog_version() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "SRPL executable plan requires procedure contract at the active catalog version",
        ));
    }

    let procedure_namespace = if ir.name.parts().len() > 1 {
        Some(ir.name.parts()[..ir.name.parts().len() - 1].join("."))
    } else {
        None
    };

    let mut binding_sources: BTreeMap<String, Vec<ColumnDescriptor>> = BTreeMap::new();
    let mut tables: BTreeSet<QualifiedName> = BTreeSet::new();
    let mut structured: BTreeSet<QualifiedName> = BTreeSet::new();
    let mut bound_objects: Vec<SrplObjectBindingEvidence> = Vec::new();
    let mut bound_operations: Vec<BoundSrplOperationPlan> =
        Vec::with_capacity(ir.body.operations.len());

    for operation in &ir.body.operations {
        match &operation.kind {
            SrplBusinessOperationKindIr::Read {
                source,
                binding,
                cardinality,
                predicates,
            } => {
                let table = ensure_table_bound(catalog, source, &mut tables, &mut bound_objects)?;
                if binding_sources.contains_key(binding) {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Srpl,
                        "SRPL read binding names must be unique",
                    ));
                }
                binding_sources.insert(binding.clone(), table.columns.clone());
                validate_predicates(predicates, &ir.inputs, &binding_sources)?;
                bound_operations.push(BoundSrplOperationPlan::ReadTable {
                    ordinal: operation.ordinal,
                    source: table.object,
                    binding: binding.clone(),
                    cardinality: *cardinality,
                    predicates: predicates.clone(),
                });
            },
            SrplBusinessOperationKindIr::Assert {
                predicate,
                failure_code,
            } => {
                validate_predicate(predicate, &ir.inputs, &binding_sources)?;
                bound_operations.push(BoundSrplOperationPlan::Assert {
                    ordinal: operation.ordinal,
                    predicate: predicate.clone(),
                    failure_code: failure_code.clone(),
                });
            },
            SrplBusinessOperationKindIr::Update {
                target,
                predicates,
                assignments,
                affected_rows_exact,
            } => {
                let table = ensure_table_bound(catalog, target, &mut tables, &mut bound_objects)?;
                validate_predicates(predicates, &ir.inputs, &binding_sources)?;
                for assignment in assignments {
                    require_column(&table.columns, &assignment.field, "SRPL update assignment")?;
                    validate_value(&assignment.value, &ir.inputs, &binding_sources)?;
                }
                if let Some(rows) = affected_rows_exact
                    && *rows == 0
                {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Srpl,
                        "SRPL update affected rows must be greater than zero",
                    ));
                }
                bound_operations.push(BoundSrplOperationPlan::UpdateTable {
                    ordinal: operation.ordinal,
                    target: table.object,
                    predicates: predicates.clone(),
                    assignments: assignments.clone(),
                    affected_rows_exact: *affected_rows_exact,
                });
            },
            SrplBusinessOperationKindIr::Emit { stream, values } => {
                let result = ir
                    .result_streams
                    .iter()
                    .find(|result| result.name == *stream)
                    .ok_or_else(|| {
                        AndromedaError::new(
                            AndromedaErrorKind::Srpl,
                            "SRPL emit references an unknown result stream",
                        )
                    })?;
                if values.len() != result.columns.len()
                    || !values
                        .iter()
                        .zip(result.columns.iter())
                        .all(|(value, column)| value.column == column.name)
                {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Srpl,
                        "SRPL emit values must match result columns exactly and in order",
                    ));
                }

                if let Some(namespace) = procedure_namespace.as_ref() {
                    let candidate_name = QualifiedName::parse(&format!("{namespace}.{stream}"));
                    if let Ok(name) = candidate_name
                        && !structured.contains(&name)
                        && let Some(object) = catalog.try_lookup_structured_object(&name)?
                    {
                        require_emit_columns_present_in_structured_object(values, &object)?;
                        structured.insert(name);
                        bound_objects.push(SrplObjectBindingEvidence {
                            object: object.object,
                            shape_hash: object.shape_hash,
                            kind: ObjectKind::StructuredObject,
                        });
                    }
                }

                for value in values {
                    require_column(&result.columns, &value.column, "SRPL emit value")?;
                    validate_value(&value.value, &ir.inputs, &binding_sources)?;
                }
                bound_operations.push(BoundSrplOperationPlan::Emit {
                    ordinal: operation.ordinal,
                    stream: stream.clone(),
                    values: values.clone(),
                });
            },
            SrplBusinessOperationKindIr::Raise { code } => {
                bound_operations.push(BoundSrplOperationPlan::Raise {
                    ordinal: operation.ordinal,
                    code: code.clone(),
                });
            },
        }
    }

    validate_result_emission(ir, &bound_operations)?;

    let plan = ExecutableProcedurePlan {
        procedure_name: ir.name.clone(),
        body: BoundSrplBodyPlan {
            operations: bound_operations,
        },
        evidence: SrplCatalogBindingEvidence {
            catalog_version: catalog.catalog_version(),
            procedure_object: procedure.object.clone(),
            procedure_contract: procedure.as_ref(),
            bound_objects,
        },
    };
    plan.validate()?;
    Ok(plan)
}

pub fn inventory_reserve_stock_body_ir() -> AndromedaResult<SrplProcedureBodyIr> {
    let body = SrplProcedureBodyIr {
        operations: vec![
            SrplBusinessOperationIr {
                ordinal: 0,
                kind: SrplBusinessOperationKindIr::Read {
                    source: QualifiedName::parse("Inventory.ProductStock")?,
                    binding: "Stock".to_string(),
                    cardinality: Cardinality::One,
                    predicates: vec![SrplPredicateIr::InputEqualsField {
                        input: "ProductId".to_string(),
                        binding: "Stock".to_string(),
                        field: "ProductId".to_string(),
                    }],
                },
            },
            SrplBusinessOperationIr {
                ordinal: 1,
                kind: SrplBusinessOperationKindIr::Assert {
                    predicate: SrplPredicateIr::FieldGreaterThanOrEqualInput {
                        binding: "Stock".to_string(),
                        field: "AvailableQuantity".to_string(),
                        input: "Quantity".to_string(),
                    },
                    failure_code: "InsufficientStock".to_string(),
                },
            },
            SrplBusinessOperationIr {
                ordinal: 2,
                kind: SrplBusinessOperationKindIr::Update {
                    target: QualifiedName::parse("Inventory.ProductStock")?,
                    predicates: vec![SrplPredicateIr::InputEqualsField {
                        input: "ProductId".to_string(),
                        binding: "Stock".to_string(),
                        field: "ProductId".to_string(),
                    }],
                    assignments: vec![SrplAssignmentIr {
                        field: "AvailableQuantity".to_string(),
                        value: SrplValueIr::SubtractInput {
                            binding: "Stock".to_string(),
                            field: "AvailableQuantity".to_string(),
                            input: "Quantity".to_string(),
                        },
                    }],
                    affected_rows_exact: Some(1),
                },
            },
            SrplBusinessOperationIr {
                ordinal: 3,
                kind: SrplBusinessOperationKindIr::Emit {
                    stream: "Reservation".to_string(),
                    values: vec![SrplEmitValueIr {
                        column: "Reserved".to_string(),
                        value: SrplValueIr::bool(true),
                    }],
                },
            },
        ],
    };
    body.validate_bounded()?;
    Ok(body)
}

fn ensure_table_bound(
    catalog: &impl SrplExecutableCatalogView,
    name: &QualifiedName,
    tables: &mut BTreeSet<QualifiedName>,
    bound_objects: &mut Vec<SrplObjectBindingEvidence>,
) -> AndromedaResult<SrplCatalogTableBinding> {
    let table = catalog.lookup_table(name)?;
    if tables.insert(name.clone()) {
        bound_objects.push(SrplObjectBindingEvidence {
            object: table.object.clone(),
            shape_hash: table.shape_hash,
            kind: ObjectKind::Table,
        });
    }
    Ok(table)
}

fn require_emit_columns_present_in_structured_object(
    values: &[SrplEmitValueIr],
    object: &SrplCatalogStructuredObjectBinding,
) -> AndromedaResult<()> {
    for value in values {
        require_column(
            &object.fields,
            &value.column,
            "SRPL emit value structured object",
        )?;
    }
    Ok(())
}

fn validate_signature_matches_contract(
    ir: &SrplProcedureIr,
    contract: &ProcedureContract,
) -> AndromedaResult<()> {
    if ir.inputs != contract.inputs {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "SRPL procedure inputs do not match the bound procedure contract",
        ));
    }

    if ir.result_streams.len() != contract.result_streams.len()
        || !ir
            .result_streams
            .iter()
            .zip(contract.result_streams.iter())
            .all(|(ir_stream, contract_stream)| {
                ir_stream.name == contract_stream.name
                    && ir_stream.columns == contract_stream.columns
                    && ResultStreamCardinality::from(ir_stream.cardinality)
                        == contract_stream.cardinality
            })
    {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "SRPL procedure results do not match the bound procedure contract",
        ));
    }

    Ok(())
}

fn validate_result_emission(
    ir: &SrplProcedureIr,
    operations: &[BoundSrplOperationPlan],
) -> AndromedaResult<()> {
    for result in &ir.result_streams {
        let emits = operations
            .iter()
            .filter(|operation| {
                matches!(
                    operation,
                    BoundSrplOperationPlan::Emit { stream, .. } if stream == &result.name
                )
            })
            .count();
        if !result.cardinality.permits_emit_operation_count(emits) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                result.cardinality.emit_count_diagnostic(),
            ));
        }
    }

    Ok(())
}

fn validate_predicates(
    predicates: &[SrplPredicateIr],
    inputs: &[ColumnDescriptor],
    binding_sources: &BTreeMap<String, Vec<ColumnDescriptor>>,
) -> AndromedaResult<()> {
    for predicate in predicates {
        validate_predicate(predicate, inputs, binding_sources)?;
    }
    Ok(())
}

fn validate_predicate(
    predicate: &SrplPredicateIr,
    inputs: &[ColumnDescriptor],
    binding_sources: &BTreeMap<String, Vec<ColumnDescriptor>>,
) -> AndromedaResult<()> {
    match predicate {
        SrplPredicateIr::InputEqualsField {
            input,
            binding,
            field,
        }
        | SrplPredicateIr::FieldGreaterThanOrEqualInput {
            binding,
            field,
            input,
        } => {
            require_input(inputs, input)?;
            let columns = binding_sources.get(binding).ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Srpl,
                    "SRPL predicate references an unbound read binding",
                )
            })?;
            require_column(columns, field, "SRPL predicate field")?;
        },
    }
    Ok(())
}

fn validate_value(
    value: &SrplValueIr,
    inputs: &[ColumnDescriptor],
    binding_sources: &BTreeMap<String, Vec<ColumnDescriptor>>,
) -> AndromedaResult<()> {
    match value {
        SrplValueIr::Input(input) => require_input(inputs, input)?,
        SrplValueIr::Field { binding, field }
        | SrplValueIr::SubtractInput { binding, field, .. } => {
            let columns = binding_sources.get(binding).ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Srpl,
                    "SRPL value references an unbound read binding",
                )
            })?;
            require_column(columns, field, "SRPL value field")?;
            if let SrplValueIr::SubtractInput { input, .. } = value {
                require_input(inputs, input)?;
            }
        },
        SrplValueIr::Constant(literal) => literal.validate()?,
        SrplValueIr::BinaryArith { left, right, .. } => {
            validate_value(left, inputs, binding_sources)?;
            validate_value(right, inputs, binding_sources)?;
        },
    }
    Ok(())
}

fn require_input(inputs: &[ColumnDescriptor], name: &str) -> AndromedaResult<()> {
    if inputs.iter().any(|input| input.name == name) {
        Ok(())
    } else {
        Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            "SRPL body references an unknown procedure input",
        ))
    }
}

fn require_column(columns: &[ColumnDescriptor], name: &str, context: &str) -> AndromedaResult<()> {
    if columns.iter().any(|column| column.name == name) {
        Ok(())
    } else {
        Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            format!("{context} references an unknown field"),
        ))
    }
}
