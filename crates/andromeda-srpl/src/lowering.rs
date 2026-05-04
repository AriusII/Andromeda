use andromeda_catalog::{
    inventory_reserve_stock_contract_candidate, CatalogDefinition, CatalogObjectRef,
    CatalogSnapshot, DefinitionBatch, DefinitionBatchId, DefinitionOperation, ObjectKind,
    ProcedureContract, ProcedureContractCandidate, QualifiedName, ResultStreamContract,
    StructuredObjectDefinition, TableDefinition,
};
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ColumnDescriptor,
    DatabaseId, NamespaceId,
};
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    BoundProcedure, BoundSrplBodyPlan, BoundSrplOperationPlan, BusinessOperationKindAst,
    ExecutableProcedurePlan, ProcedureAst, ProcedureBodyAst, SrplAssignmentIr,
    SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplCatalogBindingEvidence,
    SrplDiagnostic, SrplEmitValueIr, SrplObjectBindingEvidence, SrplPredicateIr,
    SrplProcedureBodyIr, SrplProcedureContractMetadata, SrplProcedureIr, SrplResultStreamIr,
    SrplValueIr,
};

pub fn lower_bound_procedure(bound: BoundProcedure) -> AndromedaResult<SrplProcedureIr> {
    bound.signature.validate()?;
    let body = lower_body_ast(bound.body)?;
    Ok(SrplProcedureIr {
        name: bound.signature.name,
        inputs: bound.signature.accepts,
        result_streams: bound
            .signature
            .returns
            .into_iter()
            .map(|result| SrplResultStreamIr {
                name: result.name,
                cardinality: result.cardinality,
                columns: result.columns,
            })
            .collect(),
        body,
    })
}

pub fn lower_body_ast(body: ProcedureBodyAst) -> AndromedaResult<SrplProcedureBodyIr> {
    let mut operations = Vec::new();
    for operation in body.operations {
        match operation.kind {
            BusinessOperationKindAst::Read {
                source,
                binding,
                cardinality,
            } => operations.push(SrplBusinessOperationIr {
                ordinal: operations.len() as u32,
                kind: SrplBusinessOperationKindIr::Read {
                    source: source.value,
                    binding: binding.value,
                    cardinality: cardinality.value,
                    predicates: Vec::new(),
                },
            }),
            BusinessOperationKindAst::Assert {
                predicate,
                failure_code,
            } => operations.push(SrplBusinessOperationIr {
                ordinal: operations.len() as u32,
                kind: SrplBusinessOperationKindIr::Assert {
                    predicate: SrplPredicateIr::InputEqualsField {
                        input: predicate.value,
                        binding: "scope".to_string(),
                        field: "value".to_string(),
                    },
                    failure_code: failure_code.value,
                },
            }),
            BusinessOperationKindAst::Update {
                target,
                mutation,
                affected_rows_exact,
            } => operations.push(SrplBusinessOperationIr {
                ordinal: operations.len() as u32,
                kind: SrplBusinessOperationKindIr::Update {
                    target: target.value,
                    predicates: Vec::new(),
                    assignments: vec![SrplAssignmentIr {
                        field: mutation.value,
                        value: SrplValueIr::Input("value".to_string()),
                    }],
                    affected_rows_exact: affected_rows_exact.map(|rows| rows.value),
                },
            }),
            BusinessOperationKindAst::Emit { stream, values } => {
                operations.push(SrplBusinessOperationIr {
                    ordinal: operations.len() as u32,
                    kind: SrplBusinessOperationKindIr::Emit {
                        stream: stream.value,
                        values: values
                            .into_iter()
                            .map(|value| SrplEmitValueIr {
                                column: value.value,
                                value: SrplValueIr::Bool(true),
                            })
                            .collect(),
                    },
                })
            }
            BusinessOperationKindAst::Raise { code } => operations.push(SrplBusinessOperationIr {
                ordinal: operations.len() as u32,
                kind: SrplBusinessOperationKindIr::Raise { code: code.value },
            }),
            BusinessOperationKindAst::Ensure {
                source,
                binding,
                lookup_input,
                lookup_field,
                quantity_field,
                quantity_input,
                failure_code,
            } => {
                operations.push(SrplBusinessOperationIr {
                    ordinal: operations.len() as u32,
                    kind: SrplBusinessOperationKindIr::Read {
                        source: source.value,
                        binding: binding.value.clone(),
                        cardinality: crate::Cardinality::One,
                        predicates: vec![SrplPredicateIr::InputEqualsField {
                            input: lookup_input.value,
                            binding: binding.value.clone(),
                            field: lookup_field.value,
                        }],
                    },
                });
                operations.push(SrplBusinessOperationIr {
                    ordinal: operations.len() as u32,
                    kind: SrplBusinessOperationKindIr::Assert {
                        predicate: SrplPredicateIr::FieldGreaterThanOrEqualInput {
                            binding: binding.value,
                            field: quantity_field.value,
                            input: quantity_input.value,
                        },
                        failure_code: failure_code.value,
                    },
                });
            }
            BusinessOperationKindAst::UpdateSet {
                target,
                field,
                value_binding,
                value_field,
                value_input,
                where_input,
                where_binding,
                where_field,
                affected_rows_exact,
            } => operations.push(SrplBusinessOperationIr {
                ordinal: operations.len() as u32,
                kind: SrplBusinessOperationKindIr::Update {
                    target: target.value,
                    predicates: vec![SrplPredicateIr::InputEqualsField {
                        input: where_input.value,
                        binding: where_binding.value,
                        field: where_field.value,
                    }],
                    assignments: vec![SrplAssignmentIr {
                        field: field.value,
                        value: SrplValueIr::SubtractInput {
                            binding: value_binding.value,
                            field: value_field.value,
                            input: value_input.value,
                        },
                    }],
                    affected_rows_exact: Some(affected_rows_exact.value),
                },
            }),
            BusinessOperationKindAst::Return { stream, values } => {
                operations.push(SrplBusinessOperationIr {
                    ordinal: operations.len() as u32,
                    kind: SrplBusinessOperationKindIr::Emit {
                        stream: stream.value,
                        values: values
                            .into_iter()
                            .map(|value| SrplEmitValueIr {
                                column: value.value,
                                value: SrplValueIr::Bool(true),
                            })
                            .collect(),
                    },
                })
            }
        }
    }
    let ir = SrplProcedureBodyIr { operations };
    ir.validate_bounded()?;
    Ok(ir)
}

pub fn compile_narrow_procedure_signature(
    source: &str,
) -> Result<SrplProcedureIr, crate::SrplDiagnostic> {
    let srpl_source = crate::SrplSource::new(source);
    if let Some(diagnostic) = srpl_source
        .forbidden_construct_diagnostics()
        .into_iter()
        .next()
    {
        return Err(diagnostic);
    }

    let ast = crate::parse_procedure_signature(source)?;
    validate_ast_names_for_diagnostics(&ast)?;
    let bound = crate::bind_procedure(ast).map_err(|error| {
        crate::SrplDiagnostic::new(crate::DiagnosticPhase::Binding, None, error.to_string())
    })?;
    lower_bound_procedure(bound).map_err(|error| {
        crate::SrplDiagnostic::new(crate::DiagnosticPhase::IrLowering, None, error.to_string())
    })
}

pub fn lower_ir_to_contract_candidate(
    ir: SrplProcedureIr,
    metadata: SrplProcedureContractMetadata,
) -> AndromedaResult<ProcedureContractCandidate> {
    let signature = crate::ProcedureSignature {
        name: ir.name.clone(),
        accepts: ir.inputs.clone(),
        returns: ir
            .result_streams
            .iter()
            .map(|result| crate::ResultContract {
                name: result.name.clone(),
                cardinality: result.cardinality,
                columns: result.columns.clone(),
            })
            .collect(),
    };
    signature.validate()?;
    ir.body.validate_bounded()?;
    validate_declared_error_codes(&ir.body, &metadata.error_policy.allowed_error_codes)?;

    let candidate = ProcedureContractCandidate {
        object: CatalogObjectRef {
            object_id: metadata.object_id,
            name: ir.name,
            kind: ObjectKind::Procedure,
            catalog_version: metadata.catalog_version,
        },
        procedure_id: metadata.procedure_id,
        stats_version: metadata.stats_version,
        protocol_layout: metadata.protocol_layout,
        inputs: ir.inputs,
        structured_inputs: metadata.structured_inputs,
        result_streams: ir
            .result_streams
            .into_iter()
            .enumerate()
            .map(|(index, result)| ResultStreamContract {
                stream_id: (index as u64) + 1,
                name: result.name,
                columns: result.columns,
                row_count_exact_required: result.cardinality.requires_exact_row_count(),
            })
            .collect(),
        required_permissions: metadata.required_permissions,
        transaction_policy: metadata.transaction_policy,
        compatibility_policy: metadata.compatibility_policy,
        result_metadata_policy: metadata.result_metadata_policy,
        error_policy: metadata.error_policy,
        multi_result_policy: metadata.multi_result_policy,
    };
    candidate.clone().materialize()?;
    Ok(candidate)
}

pub fn compile_narrow_procedure_contract_candidate(
    source: &str,
    metadata: SrplProcedureContractMetadata,
) -> Result<ProcedureContractCandidate, crate::SrplDiagnostic> {
    let ir = compile_narrow_procedure_signature(source)?;
    lower_ir_to_contract_candidate(ir, metadata).map_err(|error| {
        crate::SrplDiagnostic::new(crate::DiagnosticPhase::IrLowering, None, error.to_string())
    })
}

pub fn lower_ir_to_catalog_definition(
    ir: SrplProcedureIr,
    metadata: SrplProcedureContractMetadata,
) -> AndromedaResult<CatalogDefinition> {
    let contract = lower_ir_to_contract_candidate(ir, metadata)?.materialize()?;
    Ok(CatalogDefinition::Procedure(contract))
}

pub fn compile_narrow_procedure_definition(
    source: &str,
    metadata: SrplProcedureContractMetadata,
) -> Result<CatalogDefinition, crate::SrplDiagnostic> {
    let ir = compile_narrow_procedure_signature(source)?;
    lower_ir_to_catalog_definition(ir, metadata).map_err(|error| {
        crate::SrplDiagnostic::new(crate::DiagnosticPhase::IrLowering, None, error.to_string())
    })
}

pub fn compile_narrow_procedure_definition_batch(
    source: &str,
    metadata: SrplProcedureContractMetadata,
    batch_id: DefinitionBatchId,
    database_id: DatabaseId,
    namespace_id: NamespaceId,
    base_version: CatalogVersion,
) -> Result<DefinitionBatch, crate::SrplDiagnostic> {
    let definition = compile_narrow_procedure_definition(source, metadata)?;
    Ok(DefinitionBatch {
        batch_id,
        database_id,
        namespace_id,
        base_version,
        operations: vec![DefinitionOperation::Create(definition)],
    })
}

pub const INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE: &str = "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) begin ensure Inventory.ProductStock Stock where ProductId = Stock.ProductId and Stock.AvailableQuantity >= Quantity else fail InsufficientStock; update Inventory.ProductStock set AvailableQuantity = Stock.AvailableQuantity - Quantity where ProductId = Stock.ProductId affected rows 1; return Reservation (Reserved); end;";

pub fn inventory_reserve_stock_contract_metadata(
    catalog_version: CatalogVersion,
) -> SrplProcedureContractMetadata {
    let fixture = inventory_reserve_stock_contract_candidate(catalog_version);
    SrplProcedureContractMetadata {
        object_id: fixture.object.object_id,
        procedure_id: fixture.procedure_id,
        catalog_version: fixture.object.catalog_version,
        stats_version: fixture.stats_version,
        protocol_layout: fixture.protocol_layout,
        structured_inputs: fixture.structured_inputs,
        required_permissions: fixture.required_permissions,
        transaction_policy: fixture.transaction_policy,
        compatibility_policy: fixture.compatibility_policy,
        result_metadata_policy: fixture.result_metadata_policy,
        error_policy: fixture.error_policy,
        multi_result_policy: fixture.multi_result_policy,
    }
}

pub fn compile_inventory_reserve_stock_contract_candidate(
    catalog_version: CatalogVersion,
) -> Result<ProcedureContractCandidate, crate::SrplDiagnostic> {
    compile_narrow_procedure_contract_candidate(
        INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE,
        inventory_reserve_stock_contract_metadata(catalog_version),
    )
}

pub fn compile_inventory_reserve_stock_contract(
    catalog_version: CatalogVersion,
) -> Result<ProcedureContract, crate::SrplDiagnostic> {
    compile_inventory_reserve_stock_contract_candidate(catalog_version)?
        .materialize()
        .map_err(|error| {
            crate::SrplDiagnostic::new(crate::DiagnosticPhase::IrLowering, None, error.to_string())
        })
}

fn validate_declared_error_codes(
    body: &SrplProcedureBodyIr,
    declared_error_codes: &[String],
) -> AndromedaResult<()> {
    let declared = declared_error_codes
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for operation in &body.operations {
        let failure_code = match &operation.kind {
            SrplBusinessOperationKindIr::Assert { failure_code, .. } => failure_code,
            SrplBusinessOperationKindIr::Raise { code } => code,
            _ => continue,
        };

        if !declared.contains(failure_code.as_str()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "SRPL body error code is not declared by the procedure error policy",
            ));
        }
    }

    Ok(())
}

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

    let product_stock_name = QualifiedName::parse("Inventory.ProductStock")?;
    let reservation_name = QualifiedName::parse("Inventory.Reservation")?;
    let stock_definition = lookup_table(catalog, &product_stock_name)?;
    let reservation_definition = lookup_structured_object(catalog, &reservation_name)?;

    validate_inventory_operation_order(&ir.body)?;
    let bound_operations = bind_body_operations(ir, stock_definition, reservation_definition)?;
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
            stock_object: SrplObjectBindingEvidence {
                object: stock_definition.object.clone(),
                shape_hash: stock_definition.shape_hash(),
            },
            reservation_object: SrplObjectBindingEvidence {
                object: reservation_definition.object.clone(),
                shape_hash: reservation_definition.shape_hash(),
            },
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

fn lookup_structured_object<'a>(
    catalog: &'a CatalogSnapshot,
    name: &QualifiedName,
) -> AndromedaResult<&'a StructuredObjectDefinition> {
    match catalog.get_by_name(name) {
        Some(CatalogDefinition::StructuredObject(object))
            if catalog.is_active_object(object.object.object_id) =>
        {
            Ok(object)
        }
        Some(CatalogDefinition::StructuredObject(_)) => Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "SRPL executable plan cannot bind a deprecated structured object",
        )),
        Some(_) => Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "SRPL result object name resolved to a non-structured object",
        )),
        None => Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "SRPL executable plan cannot bind missing result object",
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
                    && ir_stream.cardinality.requires_exact_row_count()
                        == contract_stream.row_count_exact_required
            })
    {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "SRPL procedure results do not match the bound procedure contract",
        ));
    }

    Ok(())
}

fn validate_inventory_operation_order(body: &SrplProcedureBodyIr) -> AndromedaResult<()> {
    if body.operations.len() != 4 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            "Inventory.ReserveStock executable plan requires exactly read/assert/update/emit",
        ));
    }

    let valid_order = matches!(
        &body.operations[0].kind,
        SrplBusinessOperationKindIr::Read { .. }
    ) && matches!(
        &body.operations[1].kind,
        SrplBusinessOperationKindIr::Assert { .. }
    ) && matches!(
        &body.operations[2].kind,
        SrplBusinessOperationKindIr::Update { .. }
    ) && matches!(
        &body.operations[3].kind,
        SrplBusinessOperationKindIr::Emit { .. }
    );
    if !valid_order {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            "unsupported SRPL executable operation order for Inventory.ReserveStock",
        ));
    }

    Ok(())
}

fn bind_body_operations(
    ir: &SrplProcedureIr,
    stock: &TableDefinition,
    reservation: &StructuredObjectDefinition,
) -> AndromedaResult<Vec<BoundSrplOperationPlan>> {
    let mut binding_sources: BTreeMap<String, Vec<ColumnDescriptor>> = BTreeMap::new();
    let mut bound = Vec::with_capacity(ir.body.operations.len());

    for operation in &ir.body.operations {
        match &operation.kind {
            SrplBusinessOperationKindIr::Read {
                source,
                binding,
                cardinality,
                predicates,
            } => {
                if source != &stock.object.name {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "SRPL read source is not bound to Inventory.ProductStock",
                    ));
                }
                if binding_sources.contains_key(binding) {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Srpl,
                        "SRPL read binding names must be unique",
                    ));
                }
                binding_sources.insert(binding.clone(), stock.columns.clone());
                validate_predicates(predicates, &ir.inputs, &binding_sources)?;
                bound.push(BoundSrplOperationPlan::ReadTable {
                    ordinal: operation.ordinal,
                    source: stock.object.clone(),
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
                bound.push(BoundSrplOperationPlan::Assert {
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
                if target != &stock.object.name {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "SRPL update target is not bound to Inventory.ProductStock",
                    ));
                }
                validate_predicates(predicates, &ir.inputs, &binding_sources)?;
                for assignment in assignments {
                    require_column(&stock.columns, &assignment.field, "SRPL update assignment")?;
                    validate_value(&assignment.value, &ir.inputs, &binding_sources)?;
                }
                if affected_rows_exact != &Some(1) {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Srpl,
                        "Inventory.ReserveStock update must declare exactly one affected row",
                    ));
                }
                bound.push(BoundSrplOperationPlan::UpdateTable {
                    ordinal: operation.ordinal,
                    target: stock.object.clone(),
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
                for value in values {
                    require_column(&result.columns, &value.column, "SRPL emit value")?;
                    require_column(
                        &reservation.fields,
                        &value.column,
                        "SRPL reservation result",
                    )?;
                    validate_value(&value.value, &ir.inputs, &binding_sources)?;
                }
                bound.push(BoundSrplOperationPlan::Emit {
                    ordinal: operation.ordinal,
                    stream: stream.clone(),
                    values: values.clone(),
                });
            }
            SrplBusinessOperationKindIr::Raise { code } => {
                bound.push(BoundSrplOperationPlan::Raise {
                    ordinal: operation.ordinal,
                    code: code.clone(),
                });
            }
        }
    }

    Ok(bound)
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
        if result.cardinality.requires_exact_row_count() && emits != 1 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL exact-cardinality result stream must have exactly one emit operation",
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
        }
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
        }
        SrplValueIr::Bool(_) => {}
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

fn validate_no_sql_like_symbols(ir: &SrplProcedureIr) -> AndromedaResult<()> {
    for operation in &ir.body.operations {
        match &operation.kind {
            SrplBusinessOperationKindIr::Read {
                binding,
                predicates,
                ..
            } => {
                reject_sql_like_symbol(binding)?;
                for predicate in predicates {
                    validate_predicate_symbols(predicate)?;
                }
            }
            SrplBusinessOperationKindIr::Assert {
                predicate,
                failure_code,
            } => {
                validate_predicate_symbols(predicate)?;
                reject_sql_like_symbol(failure_code)?;
            }
            SrplBusinessOperationKindIr::Update {
                predicates,
                assignments,
                affected_rows_exact: _,
                ..
            } => {
                for predicate in predicates {
                    validate_predicate_symbols(predicate)?;
                }
                for assignment in assignments {
                    reject_sql_like_symbol(&assignment.field)?;
                    validate_value_symbols(&assignment.value)?;
                }
            }
            SrplBusinessOperationKindIr::Emit { stream, values } => {
                reject_sql_like_symbol(stream)?;
                for value in values {
                    reject_sql_like_symbol(&value.column)?;
                    validate_value_symbols(&value.value)?;
                }
            }
            SrplBusinessOperationKindIr::Raise { code } => reject_sql_like_symbol(code)?,
        }
    }
    Ok(())
}

fn validate_predicate_symbols(predicate: &SrplPredicateIr) -> AndromedaResult<()> {
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
            reject_sql_like_symbol(input)?;
            reject_sql_like_symbol(binding)?;
            reject_sql_like_symbol(field)?;
        }
    }
    Ok(())
}

fn validate_value_symbols(value: &SrplValueIr) -> AndromedaResult<()> {
    match value {
        SrplValueIr::Input(input) => reject_sql_like_symbol(input)?,
        SrplValueIr::Field { binding, field } => {
            reject_sql_like_symbol(binding)?;
            reject_sql_like_symbol(field)?;
        }
        SrplValueIr::Bool(_) => {}
        SrplValueIr::SubtractInput {
            binding,
            field,
            input,
        } => {
            reject_sql_like_symbol(binding)?;
            reject_sql_like_symbol(field)?;
            reject_sql_like_symbol(input)?;
        }
    }
    Ok(())
}

fn reject_sql_like_symbol(symbol: &str) -> AndromedaResult<()> {
    let lower = symbol.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "select" | "update" | "insert" | "delete" | "merge" | "from" | "where" | "join" | "sql"
    ) {
        Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            "SRPL executable plan rejects SQL-like free-form symbols",
        ))
    } else {
        Ok(())
    }
}

fn validate_ast_names_for_diagnostics(ast: &ProcedureAst) -> Result<(), crate::SrplDiagnostic> {
    let mut parameter_names = BTreeSet::new();
    for parameter in &ast.parameters {
        if !parameter_names.insert(parameter.name.value.as_str()) {
            return Err(crate::SrplDiagnostic::new(
                crate::DiagnosticPhase::Binding,
                Some(parameter.name.span),
                "SRPL procedure input names must be unique",
            ));
        }
    }

    let mut result_names = BTreeSet::new();
    for result in &ast.results {
        if !result_names.insert(result.name.value.as_str()) {
            return Err(crate::SrplDiagnostic::new(
                crate::DiagnosticPhase::Binding,
                Some(result.name.span),
                "SRPL result stream names must be unique",
            ));
        }

        let mut column_names = BTreeSet::new();
        for column in &result.columns {
            if !column_names.insert(column.name.value.as_str()) {
                return Err(crate::SrplDiagnostic::new(
                    crate::DiagnosticPhase::Binding,
                    Some(column.name.span),
                    "SRPL result column names must be unique",
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cardinality, DiagnosticPhase};

    #[test]
    fn compiles_narrow_signature_to_ir_without_execution_surface() {
        let ir = compile_narrow_procedure_signature(
            "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);",
        )
            .unwrap();

        assert_eq!(ir.name.as_catalog_path(), "Inventory.ReserveStock");
        assert_eq!(ir.inputs.len(), 1);
        assert_eq!(ir.result_streams[0].cardinality, Cardinality::One);
        assert!(ir.body.operations.is_empty());
    }

    #[test]
    fn compile_reports_forbidden_construct_before_lowering() {
        let diagnostic = compile_narrow_procedure_signature(
            "procedure X accepts () returns R many (C bool); execute sql",
        )
        .unwrap_err();

        assert_eq!(diagnostic.phase, DiagnosticPhase::Binding);
        assert!(diagnostic.location.is_some());
    }

    #[test]
    fn compile_reports_binding_duplicate_with_span() {
        let diagnostic = compile_narrow_procedure_signature(
            "procedure Inventory.ReserveStock accepts (ProductId i64, ProductId i64) returns Reservation one (Reserved bool);",
        )
            .unwrap_err();

        assert_eq!(diagnostic.phase, DiagnosticPhase::Binding);
        assert!(diagnostic.location.is_some());
        assert!(diagnostic.message.contains("unique"));
    }

    #[test]
    fn reserve_stock_body_skeleton_models_bounded_business_operations() {
        let body = inventory_reserve_stock_body_ir().unwrap();

        assert_eq!(body.operations.len(), 4);
        assert!(body.validate_bounded().is_ok());
        assert!(matches!(
            &body.operations[0].kind,
            SrplBusinessOperationKindIr::Read { .. }
        ));
        assert!(matches!(
            &body.operations[1].kind,
            SrplBusinessOperationKindIr::Assert { .. }
        ));
        assert!(matches!(
            &body.operations[2].kind,
            SrplBusinessOperationKindIr::Update { .. }
        ));
        assert!(matches!(
            &body.operations[3].kind,
            SrplBusinessOperationKindIr::Emit { .. }
        ));

        let SrplBusinessOperationKindIr::Update { assignments, .. } = &body.operations[2].kind
        else {
            unreachable!("operation 2 is checked as update");
        };
        assert_eq!(assignments[0].field, "AvailableQuantity");
        assert!(matches!(
            &assignments[0].value,
            SrplValueIr::SubtractInput { .. }
        ));
    }

    #[test]
    fn compiles_tiny_body_syntax_to_deterministic_ir_operations() {
        let source = "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) body { read Inventory.ProductStock Stock one; assert Quantity InsufficientStock; update Inventory.ProductStock AvailableQuantity; emit Reservation (Reserved); }";
        let first = compile_narrow_procedure_signature(source).unwrap();
        let second = compile_narrow_procedure_signature(source).unwrap();

        assert_eq!(first.body, second.body);
        assert_eq!(first.body.operations.len(), 4);
        assert!(first.body.validate_bounded().is_ok());
        assert!(matches!(
            &first.body.operations[0].kind,
            SrplBusinessOperationKindIr::Read {
                binding,
                cardinality: Cardinality::One,
                ..
            } if binding.as_str() == "Stock"
        ));
        assert!(matches!(
            &first.body.operations[1].kind,
            SrplBusinessOperationKindIr::Assert {
                failure_code,
                ..
            } if failure_code.as_str() == "InsufficientStock"
        ));
        let SrplBusinessOperationKindIr::Update { assignments, .. } =
            &first.body.operations[2].kind
        else {
            panic!("operation 2 should lower to update");
        };
        assert_eq!(assignments[0].field, "AvailableQuantity");
        assert!(matches!(
            &first.body.operations[3].kind,
            SrplBusinessOperationKindIr::Emit { stream, values }
                if stream.as_str() == "Reservation"
                    && matches!(
                        values.first(),
                        Some(value) if value.column.as_str() == "Reserved"
            )
        ));
    }

    #[test]
    fn compiles_begin_end_reserve_stock_source_to_inventory_ir() {
        let source = "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) begin ensure Inventory.ProductStock Stock where ProductId = Stock.ProductId and Stock.AvailableQuantity >= Quantity else fail InsufficientStock; update Inventory.ProductStock set AvailableQuantity = Stock.AvailableQuantity - Quantity where ProductId = Stock.ProductId affected rows 1; return Reservation (Reserved); end;";
        let ir = compile_narrow_procedure_signature(source).unwrap();
        let expected = inventory_reserve_stock_body_ir().unwrap();

        assert_eq!(ir.body, expected);
    }

    #[test]
    fn body_validation_rejects_unbounded_or_sparse_operations() {
        let mut body = inventory_reserve_stock_body_ir().unwrap();
        body.operations[2].ordinal = 4;

        let error = body.validate_bounded().unwrap_err();

        assert!(error.message().contains("dense"));
    }
}
