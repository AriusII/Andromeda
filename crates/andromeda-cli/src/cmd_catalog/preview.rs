use super::types::{ColumnInfo, ProcedureContractInfo, ProcedureManifestInfo, ProcedureMetadata};

pub(super) fn preview_procedures(namespace: Option<&str>) -> Vec<ProcedureMetadata> {
    let procedures = vec![
        ProcedureMetadata {
            procedure_id: 1,
            name: "InventoryReserveStock".to_string(),
            namespace: Some("inventory".to_string()),
            contract_hash: "a1b2c3d4e5f6".to_string(),
            catalog_version: 1,
        },
        ProcedureMetadata {
            procedure_id: 2,
            name: "PaymentProcess".to_string(),
            namespace: Some("payments".to_string()),
            contract_hash: "f1e2d3c4b5a6".to_string(),
            catalog_version: 1,
        },
        ProcedureMetadata {
            procedure_id: 3,
            name: "UserAuthenticate".to_string(),
            namespace: Some("auth".to_string()),
            contract_hash: "7f8e9d0c1b2a".to_string(),
            catalog_version: 2,
        },
    ];

    if let Some(namespace) = namespace {
        procedures
            .into_iter()
            .filter(|procedure| procedure.namespace.as_deref() == Some(namespace))
            .collect()
    } else {
        procedures
    }
}

pub(super) fn preview_contract(procedure_id: u64) -> ProcedureContractInfo {
    ProcedureContractInfo {
        procedure_id,
        name: "InventoryReserveStock".to_string(),
        contract_hash: "a1b2c3d4e5f6".to_string(),
        input_columns: inventory_input_columns(),
        output_columns: vec![
            ColumnInfo {
                name: "remaining_quantity".to_string(),
                column_type: "INT32".to_string(),
                nullable: false,
            },
            ColumnInfo {
                name: "status".to_string(),
                column_type: "STRING".to_string(),
                nullable: false,
            },
        ],
        isolation_level: "Snapshot".to_string(),
        access_mode: "ReadWrite".to_string(),
    }
}

pub(super) fn preview_manifest(
    procedure_id: Option<u64>,
    qualified_name: Option<&str>,
) -> ProcedureManifestInfo {
    ProcedureManifestInfo {
        procedure_id: procedure_id.unwrap_or(1),
        qualified_name: qualified_name
            .unwrap_or("InventoryReserveStock")
            .to_string(),
        catalog_version: 1,
        min_compatible_version: 1,
        contract_hash: "a1b2c3d4e5f6".to_string(),
        input_columns: inventory_input_columns(),
        output_columns: vec![ColumnInfo {
            name: "status".to_string(),
            column_type: "STRING".to_string(),
            nullable: false,
        }],
        is_mutable: true,
    }
}

fn inventory_input_columns() -> Vec<ColumnInfo> {
    vec![
        ColumnInfo {
            name: "product_id".to_string(),
            column_type: "INT64".to_string(),
            nullable: false,
        },
        ColumnInfo {
            name: "quantity".to_string(),
            column_type: "INT32".to_string(),
            nullable: false,
        },
    ]
}
