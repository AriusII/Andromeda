use andromeda_core::{
    AndromedaResult, CatalogObjectId, CatalogVersion, ColumnDescriptor, DatabaseId, NamespaceId,
    ProcedureId, ScalarType, TypeDescriptor,
};

use crate::{
    AccessMode, CatalogBindingKind, CatalogDefinition, CatalogObjectBinding, CatalogObjectRef,
    CompatibilityPolicy, DefinitionBatch, DefinitionBatchId, DefinitionOperation, IsolationPolicy,
    MultiResultPolicy, ObjectKind, ProcedureContract, ProcedureContractCandidate,
    ProcedureErrorPolicy, ProtocolLayoutRef, QualifiedName, ResultMetadataPolicy,
    ResultStreamContract, StatsVersion, StructuredObjectDefinition, TableDefinition,
    TransactionPolicy,
};

pub const INVENTORY_RESERVE_STOCK_PERMISSION: &str = "Inventory.ReserveStock.Execute";
pub const INVENTORY_RESERVE_STOCK_PROCEDURE_ID: ProcedureId = ProcedureId::new(0x5253);
pub const INVENTORY_RESERVE_STOCK_OBJECT_ID: CatalogObjectId = CatalogObjectId::new(0x5253);
pub const INVENTORY_PRODUCT_STOCK_OBJECT_ID: CatalogObjectId = CatalogObjectId::new(0x5001);
pub const INVENTORY_RESERVATION_OBJECT_ID: CatalogObjectId = CatalogObjectId::new(0x5002);
pub const INVENTORY_DATABASE_ID: DatabaseId = DatabaseId::new(0x1000);
pub const INVENTORY_NAMESPACE_ID: NamespaceId = NamespaceId::new(0x1001);
pub const INVENTORY_DEFINITION_BATCH_ID: DefinitionBatchId = DefinitionBatchId::new(0x1002);

pub fn inventory_reserve_stock_contract() -> AndromedaResult<ProcedureContract> {
    inventory_reserve_stock_contract_candidate(CatalogVersion::new(1)).materialize()
}

pub fn inventory_reserve_stock_contract_candidate(
    catalog_version: CatalogVersion,
) -> ProcedureContractCandidate {
    ProcedureContractCandidate {
        object: CatalogObjectRef {
            object_id: INVENTORY_RESERVE_STOCK_OBJECT_ID,
            name: QualifiedName::parse("Inventory.ReserveStock")
                .expect("fixture procedure name is valid"),
            kind: ObjectKind::Procedure,
            catalog_version,
        },
        procedure_id: INVENTORY_RESERVE_STOCK_PROCEDURE_ID,
        stats_version: StatsVersion::new(1),
        protocol_layout: inventory_protocol_layout_ref(),
        inputs: vec![
            phase1_column("ProductId", ScalarType::I64, 0),
            phase1_column("Quantity", ScalarType::I64, 1),
        ],
        structured_inputs: Vec::new(),
        result_streams: vec![ResultStreamContract {
            stream_id: 1,
            name: "Reservation".to_string(),
            columns: vec![phase1_column("Reserved", ScalarType::Bool, 0)],
            row_count_exact_required: true,
        }],
        required_permissions: vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Serializable,
            retryable: false,
        },
        compatibility_policy: CompatibilityPolicy::ExactHash,
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: true,
            allowed_error_codes: vec!["InsufficientStock".to_string()],
        },
        multi_result_policy: MultiResultPolicy::SingleResultOnly,
    }
}

pub fn inventory_protocol_layout_ref() -> ProtocolLayoutRef {
    ProtocolLayoutRef {
        descriptor_set_hash: andromeda_core::ContractHash::test_vector(0x51),
        frame_envelope_hash: andromeda_core::ContractHash::test_vector(0x52),
    }
}

pub fn inventory_product_stock_table(
    catalog_version: CatalogVersion,
) -> AndromedaResult<TableDefinition> {
    Ok(TableDefinition {
        object: CatalogObjectRef {
            object_id: INVENTORY_PRODUCT_STOCK_OBJECT_ID,
            name: QualifiedName::parse("Inventory.ProductStock")?,
            kind: ObjectKind::Table,
            catalog_version,
        },
        columns: vec![
            phase1_column("ProductId", ScalarType::I64, 0),
            phase1_column("AvailableQuantity", ScalarType::I64, 1),
            phase1_column("Version", ScalarType::I64, 2),
        ],
    })
}

pub fn inventory_reservation_structured_object(
    catalog_version: CatalogVersion,
) -> AndromedaResult<StructuredObjectDefinition> {
    Ok(StructuredObjectDefinition {
        object: CatalogObjectRef {
            object_id: INVENTORY_RESERVATION_OBJECT_ID,
            name: QualifiedName::parse("Inventory.Reservation")?,
            kind: ObjectKind::StructuredObject,
            catalog_version,
        },
        fields: vec![
            phase1_column("ProductId", ScalarType::I64, 0),
            phase1_column("Quantity", ScalarType::I64, 1),
            phase1_column("RemainingQuantity", ScalarType::I64, 2),
            phase1_column("Reserved", ScalarType::Bool, 3),
        ],
        unique_by: vec!["ProductId".to_string()],
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryReserveStockCatalogBindings {
    pub procedure: CatalogObjectRef,
    pub product_stock_table: CatalogObjectRef,
    pub reservation_structured_object: CatalogObjectRef,
    pub product_stock_table_binding: CatalogObjectBinding,
    pub reservation_result_binding: CatalogObjectBinding,
}

impl InventoryReserveStockCatalogBindings {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.product_stock_table_binding.validate()?;
        self.reservation_result_binding.validate()?;

        if self.product_stock_table_binding.dependent != self.procedure
            || self.reservation_result_binding.dependent != self.procedure
        {
            return Err(andromeda_core::AndromedaError::new(
                andromeda_core::AndromedaErrorKind::Catalog,
                "Inventory.ReserveStock bindings must use the ReserveStock procedure as dependent",
            ));
        }

        if self.product_stock_table_binding.dependency != self.product_stock_table {
            return Err(andromeda_core::AndromedaError::new(
                andromeda_core::AndromedaErrorKind::Catalog,
                "Inventory.ReserveStock table binding must target Inventory.ProductStock",
            ));
        }

        if self.reservation_result_binding.dependency != self.reservation_structured_object {
            return Err(andromeda_core::AndromedaError::new(
                andromeda_core::AndromedaErrorKind::Catalog,
                "Inventory.ReserveStock result binding must target Inventory.Reservation",
            ));
        }

        Ok(())
    }
}

pub fn inventory_reserve_stock_catalog_bindings(
    catalog_version: CatalogVersion,
) -> AndromedaResult<InventoryReserveStockCatalogBindings> {
    let procedure = inventory_reserve_stock_contract_candidate(catalog_version)
        .materialize()?
        .object;
    let product_stock_table = inventory_product_stock_table(catalog_version)?.object;
    let reservation_structured_object =
        inventory_reservation_structured_object(catalog_version)?.object;

    let bindings = InventoryReserveStockCatalogBindings {
        procedure: procedure.clone(),
        product_stock_table: product_stock_table.clone(),
        reservation_structured_object: reservation_structured_object.clone(),
        product_stock_table_binding: CatalogObjectBinding {
            dependent: procedure.clone(),
            dependency: product_stock_table,
            kind: CatalogBindingKind::WritesTable,
        },
        reservation_result_binding: CatalogObjectBinding {
            dependent: procedure,
            dependency: reservation_structured_object,
            kind: CatalogBindingKind::EmitsStructuredObject,
        },
    };
    bindings.validate()?;
    Ok(bindings)
}

pub fn inventory_domain_definition_batch() -> AndromedaResult<DefinitionBatch> {
    let base_version = CatalogVersion::new(0);
    let next_version = CatalogVersion::new(1);
    Ok(DefinitionBatch {
        batch_id: INVENTORY_DEFINITION_BATCH_ID,
        database_id: INVENTORY_DATABASE_ID,
        namespace_id: INVENTORY_NAMESPACE_ID,
        base_version,
        operations: vec![
            DefinitionOperation::Create(CatalogDefinition::Table(inventory_product_stock_table(
                next_version,
            )?)),
            DefinitionOperation::Create(CatalogDefinition::StructuredObject(
                inventory_reservation_structured_object(next_version)?,
            )),
            DefinitionOperation::Create(CatalogDefinition::Procedure(
                inventory_reserve_stock_contract_candidate(next_version).materialize()?,
            )),
        ],
    })
}

fn phase1_column(name: &str, scalar: ScalarType, ordinal: u32) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(scalar),
        ordinal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::AndromedaErrorKind;

    #[test]
    fn reserve_stock_contract_helper_returns_valid_phase1_vector() {
        let contract = inventory_reserve_stock_contract().unwrap();

        assert_eq!(
            contract.object.name.as_catalog_path(),
            "Inventory.ReserveStock"
        );
        assert_eq!(contract.procedure_id, INVENTORY_RESERVE_STOCK_PROCEDURE_ID);
        assert!(!contract.contract_hash.is_zero());
        assert_eq!(contract.contract_hash, contract.canonical_hash());
        assert_eq!(contract.stats_version, StatsVersion::new(1));
        assert_eq!(contract.protocol_layout, inventory_protocol_layout_ref());
        assert_eq!(
            contract.required_permissions,
            vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()]
        );
        assert_eq!(
            contract.error_policy.allowed_error_codes,
            vec!["InsufficientStock".to_string()]
        );
        assert!(contract.as_ref().validate().is_ok());
        assert!(contract.validate().is_ok());
    }

    #[test]
    fn validated_contract_helper_surfaces_missing_permissions() {
        let mut contract = inventory_reserve_stock_contract().unwrap();
        contract.required_permissions.clear();

        let error = contract.validated().unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Security);
        assert!(error.message().contains("required permissions"));
    }

    #[test]
    fn inventory_domain_definition_batch_plans_catalog_objects_together() {
        let batch = inventory_domain_definition_batch().unwrap();
        let plan = batch.dry_run().unwrap();

        assert_eq!(plan.next_version, CatalogVersion::new(1));
        assert_eq!(plan.created_objects.len(), 3);
        assert!(
            plan.created_objects
                .iter()
                .any(|object| object.object_id == INVENTORY_PRODUCT_STOCK_OBJECT_ID)
        );
        assert!(
            plan.created_objects
                .iter()
                .any(|object| object.object_id == INVENTORY_RESERVATION_OBJECT_ID)
        );
        assert!(
            plan.created_objects
                .iter()
                .any(|object| object.object_id == INVENTORY_RESERVE_STOCK_OBJECT_ID)
        );
    }

    #[test]
    fn inventory_reserve_stock_catalog_bindings_are_exact_fixture_evidence() {
        let bindings = inventory_reserve_stock_catalog_bindings(CatalogVersion::new(1)).unwrap();

        assert_eq!(
            bindings.procedure.object_id,
            INVENTORY_RESERVE_STOCK_OBJECT_ID
        );
        assert_eq!(
            bindings.product_stock_table.object_id,
            INVENTORY_PRODUCT_STOCK_OBJECT_ID
        );
        assert_eq!(
            bindings.reservation_structured_object.object_id,
            INVENTORY_RESERVATION_OBJECT_ID
        );
        assert_eq!(
            bindings.product_stock_table_binding.kind,
            CatalogBindingKind::WritesTable
        );
        assert_eq!(
            bindings.reservation_result_binding.kind,
            CatalogBindingKind::EmitsStructuredObject
        );
        assert!(bindings.validate().is_ok());
    }

    #[test]
    fn inventory_domain_batch_contains_objects_named_by_catalog_bindings() {
        let batch = inventory_domain_definition_batch().unwrap();
        let bindings = inventory_reserve_stock_catalog_bindings(CatalogVersion::new(1)).unwrap();
        let created_objects = batch
            .operations
            .iter()
            .map(|operation| match operation {
                DefinitionOperation::Create(definition) => definition.object_ref(),
                DefinitionOperation::Deprecate(target) => &target.object,
            })
            .collect::<Vec<_>>();

        assert!(created_objects.contains(&&bindings.product_stock_table));
        assert!(created_objects.contains(&&bindings.reservation_structured_object));
        assert!(created_objects.contains(&&bindings.procedure));
    }
}
