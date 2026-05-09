use andromeda_contract::{CatalogDefinition, ObjectKind};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};
use std::collections::BTreeSet;

use crate::{
    CatalogLifecycleAction, DefinitionBatchDependencyGraphHash, DefinitionBatchId,
    DefinitionBatchSourceHash, DefinitionOperation, PlannedDefinition, PlannedLifecycleTransition,
    compute_definition_batch_source_hash,
    operation::{duplicate_lifecycle_error, duplicate_lifecycle_name_error},
    validate_in_batch_dependencies,
};

/// Report produced by a materialized SRPL Procedure dry-run validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplBatchDryRunReport {
    /// Whether all procedures in the batch are valid.
    pub all_valid: bool,
    /// Number of valid procedures.
    pub valid_count: usize,
    /// Number of rejected procedures.
    pub rejected_count: usize,
    /// Diagnostic messages for each rejected procedure.
    pub rejection_reasons: Vec<String>,
}

impl SrplBatchDryRunReport {
    /// Create a successful report for an all-valid Procedure set.
    pub fn success(valid_count: usize) -> Self {
        Self {
            all_valid: true,
            valid_count,
            rejected_count: 0,
            rejection_reasons: Vec::new(),
        }
    }

    /// Create a failure report for at least one invalid Procedure.
    pub fn failure(valid_count: usize, reasons: Vec<String>) -> Self {
        Self {
            all_valid: false,
            valid_count,
            rejected_count: reasons.len(),
            rejection_reasons: reasons,
        }
    }
}

/// Validate materialized Procedure contracts in a batch dry-run phase.
///
/// This function does not parse SRPL source. It validates only Procedure
/// contracts that have already been materialized into catalog definitions.
pub fn validate_srpl_operations_dry_run(
    operations: &[DefinitionOperation],
) -> AndromedaResult<SrplBatchDryRunReport> {
    let mut valid_count = 0;
    let mut rejection_reasons = Vec::new();

    for (operation_index, operation) in operations.iter().enumerate() {
        let DefinitionOperation::Create(CatalogDefinition::Procedure(contract)) = operation else {
            continue;
        };

        let procedure_name = contract.object.name.as_catalog_path();
        if contract.object.kind != ObjectKind::Procedure {
            rejection_reasons.push(format!(
                "operation {operation_index} procedure {procedure_name}: catalog object kind must be Procedure"
            ));
            continue;
        }

        if let Err(error) = contract.validate_canonical_hash() {
            rejection_reasons.push(format!(
                "operation {operation_index} procedure {procedure_name}: {}",
                error.message()
            ));
            continue;
        }

        if let Err(error) = contract.binding().validate() {
            rejection_reasons.push(format!(
                "operation {operation_index} procedure {procedure_name}: {}",
                error.message()
            ));
            continue;
        }

        valid_count += 1;
    }

    if rejection_reasons.is_empty() {
        return Ok(SrplBatchDryRunReport::success(valid_count));
    }

    let report = SrplBatchDryRunReport::failure(valid_count, rejection_reasons);
    Err(AndromedaError::new(
        AndromedaErrorKind::Contract,
        format!(
            "definition batch SRPL dry-run rejected {} procedure(s): {}",
            report.rejected_count,
            report.rejection_reasons.join("; ")
        ),
    ))
}

/// Portable DefinitionBatch dry-run result before catalog mutation/WAL planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionBatchDryRun {
    pub batch_id: DefinitionBatchId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub operation_count: usize,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub source_hash: DefinitionBatchSourceHash,
    pub dependency_graph_hash: DefinitionBatchDependencyGraphHash,
    pub created_objects: Vec<PlannedDefinition>,
    pub deprecated_objects: Vec<PlannedLifecycleTransition>,
}

/// Validate a DefinitionBatch without building catalog mutation WAL records.
pub fn dry_run_definition_batch(
    batch_id: DefinitionBatchId,
    database_id: DatabaseId,
    namespace_id: NamespaceId,
    base_version: CatalogVersion,
    operations: &[DefinitionOperation],
) -> AndromedaResult<DefinitionBatchDryRun> {
    if operations.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "definition batch must contain at least one operation",
        ));
    }

    if batch_id.is_zero() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "definition batch id must not be zero",
        ));
    }

    if database_id.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "definition batch database id must not be zero",
        ));
    }

    if namespace_id.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "definition batch namespace id must not be zero",
        ));
    }

    let next_version = base_version.get().checked_add(1).ok_or_else(|| {
        AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "catalog version overflow during definition batch planning",
        )
    })?;
    let next_version = CatalogVersion::new(next_version);
    validate_srpl_operations_dry_run(operations)?;

    let mut object_ids = BTreeSet::new();
    let mut object_names = BTreeSet::new();
    let mut lifecycle_target_ids = BTreeSet::new();
    let mut lifecycle_target_names = BTreeSet::new();
    let mut created_objects = Vec::with_capacity(operations.len());
    let mut deprecated_objects = Vec::new();

    for operation in operations {
        match operation {
            DefinitionOperation::Create(definition) => {
                definition.validate()?;

                let object = definition.object_ref();
                if object.catalog_version != next_version {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "created object catalog version must match the planned next catalog version",
                    ));
                }

                if lifecycle_target_ids.contains(&object.object_id)
                    || !object_ids.insert(object.object_id)
                {
                    return Err(duplicate_lifecycle_error());
                }

                if lifecycle_target_names.contains(&object.name)
                    || !object_names.insert(object.name.clone())
                {
                    return Err(duplicate_lifecycle_name_error());
                }

                created_objects.push(PlannedDefinition {
                    object_id: object.object_id,
                    name: object.name.clone(),
                    kind: object.kind,
                    planned_version: next_version,
                });
            },
            DefinitionOperation::Deprecate(target) => {
                target.validate()?;

                if target.object.catalog_version > base_version {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "deprecated object version must not be newer than the definition batch base version",
                    ));
                }

                if object_ids.contains(&target.object.object_id)
                    || !lifecycle_target_ids.insert(target.object.object_id)
                {
                    return Err(duplicate_lifecycle_error());
                }

                if object_names.contains(&target.object.name)
                    || !lifecycle_target_names.insert(target.object.name.clone())
                {
                    return Err(duplicate_lifecycle_name_error());
                }

                deprecated_objects.push(PlannedLifecycleTransition {
                    object_id: target.object.object_id,
                    name: target.object.name.clone(),
                    kind: target.object.kind,
                    action: CatalogLifecycleAction::Deprecate,
                    planned_version: next_version,
                });
            },
        }
    }

    let source_hash = compute_definition_batch_source_hash(
        batch_id,
        database_id,
        namespace_id,
        base_version,
        operations,
    );
    let dependency_graph_hash = validate_in_batch_dependencies(operations)?.dependency_graph_hash();

    Ok(DefinitionBatchDryRun {
        batch_id,
        database_id,
        namespace_id,
        operation_count: operations.len(),
        previous_version: base_version,
        next_version,
        source_hash,
        dependency_graph_hash,
        created_objects,
        deprecated_objects,
    })
}
