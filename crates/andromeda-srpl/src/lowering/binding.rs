//! Catalog binding for executable SRPL procedure plans.

use std::collections::BTreeMap;

use andromeda_catalog::{
    CatalogDefinition, CatalogSnapshot, ObjectKind, ProcedureContract, QualifiedName,
    StructuredObjectDefinition, TableDefinition,
};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, ColumnDescriptor};

use crate::{
    BoundSrplBodyPlan, BoundSrplOperationPlan, ExecutableProcedurePlan, SrplAssignmentIr,
    SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplCatalogBindingEvidence,
    SrplDiagnostic, SrplEmitValueIr, SrplObjectBindingEvidence, SrplPredicateIr,
    SrplProcedureBodyIr, SrplProcedureIr, SrplValueIr,
};

use super::validation::{
    require_column, validate_no_sql_like_symbols, validate_predicate, validate_predicates,
    validate_value,
};

pub fn bind_executable_procedure_plan(
    ir: &SrplProcedureIr,
    catalog: &CatalogSnapshot,
) -> AndromedaResult<ExecutableProcedurePlan> {
    ir.body.validate_bounded()?;
    validate_no_sql_like_symbols(ir)?;

    let procedure = lookup_procedure_contract(catalog, &ir.name)?;
    procedure.validate_canonical_hash()?;
    validate_signature_matches_contract(ir, procedure)?;

    if procedure.object.catalog_version != catalog.version {
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
    let mut tables: BTreeMap<QualifiedName, &TableDefinition> = BTreeMap::new();
    let mut structured: BTreeMap<QualifiedName, &StructuredObjectDefinition> = BTreeMap::new();
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
                    source: table.object.clone(),
                    binding: binding.clone(),
                    cardinality: *cardinality,
                    predicates: predicates.clone(),
                });
            }
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
            }
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
                    target: table.object.clone(),
                    predicates: predicates.clone(),
                    assignments: assignments.clone(),
                    affected_rows_exact: *affected_rows_exact,
                });
            }
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
                    let candidate_name =
                        QualifiedName::parse(&format!("{namespace}.{stream}")).ok();
                    if let Some(name) = candidate_name
                        && !structured.contains_key(&name)
                        && let Some(object) = try_lookup_structured_object(catalog, &name)?
                    {
                        require_emit_columns_present_in_structured_object(values, object)?;
                        structured.insert(name.clone(), object);
                        bound_objects.push(SrplObjectBindingEvidence {
                            object: object.object.clone(),
                            shape_hash: object.shape_hash(),
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
            }
            SrplBusinessOperationKindIr::Raise { code } => {
                bound_operations.push(BoundSrplOperationPlan::Raise {
                    ordinal: operation.ordinal,
                    code: code.clone(),
                });
            }
        }
    }

    validate_result_emission(ir, &bound_operations)?;

    let plan = ExecutableProcedurePlan {
        procedure_name: ir.name.clone(),
        body: BoundSrplBodyPlan {
            operations: bound_operations,
        },
        evidence: SrplCatalogBindingEvidence {
            catalog_version: catalog.version,
            procedure_object: procedure.object.clone(),
            procedure_contract: procedure.as_ref(),
            bound_objects,
        },
    };
    plan.validate()?;
    Ok(plan)
}

pub fn inventory_reserve_stock_body_ir() -> Result<SrplProcedureBodyIr, SrplDiagnostic> {
    let body = SrplProcedureBodyIr {
        operations: vec![
            SrplBusinessOperationIr {
                ordinal: 0,
                kind: SrplBusinessOperationKindIr::Read {
                    source: andromeda_catalog::QualifiedName::parse("Inventory.ProductStock")
                        .map_err(|error| {
                            SrplDiagnostic::new(
                                crate::DiagnosticPhase::IrLowering,
                                None,
                                error.to_string(),
                            )
                        })?,
                    binding: "Stock".to_string(),
                    cardinality: crate::Cardinality::One,
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
                    target: andromeda_catalog::QualifiedName::parse("Inventory.ProductStock")
                        .map_err(|error| {
                            SrplDiagnostic::new(
                                crate::DiagnosticPhase::IrLowering,
                                None,
                                error.to_string(),
                            )
                        })?,
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
                        value: SrplValueIr::Bool(true),
                    }],
                },
            },
        ],
    };
    body.validate_bounded().map_err(|error| {
        SrplDiagnostic::new(crate::DiagnosticPhase::IrLowering, None, error.to_string())
    })?;
    Ok(body)
}

fn ensure_table_bound<'a>(
    catalog: &'a CatalogSnapshot,
    name: &QualifiedName,
    tables: &mut BTreeMap<QualifiedName, &'a TableDefinition>,
    bound_objects: &mut Vec<SrplObjectBindingEvidence>,
) -> AndromedaResult<&'a TableDefinition> {
    if let Some(existing) = tables.get(name) {
        return Ok(*existing);
    }
    let table = lookup_table(catalog, name)?;
    tables.insert(name.clone(), table);
    bound_objects.push(SrplObjectBindingEvidence {
        object: table.object.clone(),
        shape_hash: table.shape_hash(),
        kind: ObjectKind::Table,
    });
    Ok(table)
}

fn try_lookup_structured_object<'a>(
    catalog: &'a CatalogSnapshot,
    name: &QualifiedName,
) -> AndromedaResult<Option<&'a StructuredObjectDefinition>> {
    match catalog.get_by_name(name) {
        Some(CatalogDefinition::StructuredObject(object))
            if catalog.is_active_object(object.object.object_id) =>
        {
            Ok(Some(object))
        }
        Some(CatalogDefinition::StructuredObject(_)) => Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "SRPL executable plan cannot bind a deprecated structured object",
        )),
        Some(_) | None => Ok(None),
    }
}

fn require_emit_columns_present_in_structured_object(
    values: &[SrplEmitValueIr],
    object: &StructuredObjectDefinition,
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

fn lookup_procedure_contract<'a>(
    catalog: &'a CatalogSnapshot,
    name: &QualifiedName,
) -> AndromedaResult<&'a ProcedureContract> {
    match catalog.get_by_name(name) {
        Some(CatalogDefinition::Procedure(contract))
            if catalog.is_active_object(contract.object.object_id) =>
        {
            Ok(contract)
        }
        Some(CatalogDefinition::Procedure(_)) => Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "SRPL executable plan cannot bind a deprecated procedure contract",
        )),
        Some(_) => Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "SRPL executable plan procedure name resolved to a non-procedure object",
        )),
        None => Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "SRPL executable plan cannot bind missing procedure contract",
        )),
    }
}

fn lookup_table<'a>(
    catalog: &'a CatalogSnapshot,
    name: &QualifiedName,
) -> AndromedaResult<&'a TableDefinition> {
    match catalog.get_by_name(name) {
        Some(CatalogDefinition::Table(table))
            if catalog.is_active_object(table.object.object_id) =>
        {
            Ok(table)
        }
        Some(CatalogDefinition::Table(_)) => Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "SRPL executable plan cannot bind a deprecated table",
        )),
        Some(_) => Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "SRPL body table name resolved to a non-table object",
        )),
        None => Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "SRPL body references an unbound table/object name",
        )),
    }
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
                    && andromeda_catalog::ResultStreamCardinality::from(ir_stream.cardinality)
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
