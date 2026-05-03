use andromeda_core::{
    AndromedaResult, CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, ProcedureId,
    ScalarType, TypeDescriptor,
};

use crate::{
    AccessMode, CatalogObjectRef, CompatibilityPolicy, IsolationPolicy, ObjectKind,
    ProcedureContract, QualifiedName, ResultStreamContract, TransactionPolicy,
};

pub const INVENTORY_RESERVE_STOCK_PERMISSION: &str = "Inventory.ReserveStock.Execute";
pub const INVENTORY_RESERVE_STOCK_CONTRACT_HASH: ContractHash = ContractHash::test_vector(0x52);
pub const INVENTORY_RESERVE_STOCK_PROCEDURE_ID: ProcedureId = ProcedureId::new(0x5253);
pub const INVENTORY_RESERVE_STOCK_OBJECT_ID: CatalogObjectId = CatalogObjectId::new(0x5253);

pub fn inventory_reserve_stock_contract() -> AndromedaResult<ProcedureContract> {
    ProcedureContract {
        object: CatalogObjectRef {
            object_id: INVENTORY_RESERVE_STOCK_OBJECT_ID,
            name: QualifiedName::parse("Inventory.ReserveStock")?,
            kind: ObjectKind::Procedure,
            catalog_version: CatalogVersion::new(1),
        },
        procedure_id: INVENTORY_RESERVE_STOCK_PROCEDURE_ID,
        contract_hash: INVENTORY_RESERVE_STOCK_CONTRACT_HASH,
        inputs: vec![
            phase1_column("ProductId", ScalarType::I64, 0),
            phase1_column("Quantity", ScalarType::I64, 1),
        ],
        structured_inputs: Vec::new(),
        result_streams: vec![ResultStreamContract {
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
    }
    .validated()
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
        assert_eq!(
            contract.contract_hash,
            INVENTORY_RESERVE_STOCK_CONTRACT_HASH
        );
        assert!(!contract.contract_hash.is_zero());
        assert_eq!(
            contract.required_permissions,
            vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()]
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
}
