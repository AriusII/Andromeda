use andromeda_catalog::{CatalogDefinition, DefinitionBatch, DefinitionBatchId};
use andromeda_srpl::{
    SrplProcedureContractMetadata,
    definition_batch_bridge::{
        SrplDefinitionBatchDryRunRequest, SrplDefinitionBatchProcedureSource,
        SrplProcedureDefinition,
    },
    inventory_reserve_stock_contract_metadata,
};
use andromeda_types::{CatalogObjectId, CatalogVersion, DatabaseId, NamespaceId, ProcedureId};

pub(crate) const TEST_DB_ID: DatabaseId = DatabaseId::new(1);
pub(crate) const TEST_NS_ID: NamespaceId = NamespaceId::new(1);

pub(crate) fn test_batch(base_version: CatalogVersion, batch_id: u64) -> DefinitionBatch {
    DefinitionBatch {
        batch_id: DefinitionBatchId::new(batch_id),
        database_id: TEST_DB_ID,
        namespace_id: TEST_NS_ID,
        base_version,
        operations: Vec::new(),
    }
}

pub(crate) fn signature_only_source() -> String {
    "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);"
        .to_string()
}

pub(crate) fn lookup_signature_source() -> String {
    "procedure Inventory.LookupStock accepts (ProductId i64) returns Stock one (AvailableQuantity i64);"
        .to_string()
}

pub(crate) fn test_metadata(
    object_id: u64,
    procedure_id: u64,
    catalog_version: CatalogVersion,
) -> SrplProcedureContractMetadata {
    let mut metadata = inventory_reserve_stock_contract_metadata(catalog_version);
    metadata.object_id = CatalogObjectId::new(object_id);
    metadata.procedure_id = ProcedureId::new(procedure_id);
    metadata
}

pub(crate) fn dry_run_request(
    batch_id: u64,
    base_version: CatalogVersion,
    procedures: Vec<SrplDefinitionBatchProcedureSource>,
) -> SrplDefinitionBatchDryRunRequest {
    SrplDefinitionBatchDryRunRequest {
        batch_id: DefinitionBatchId::new(batch_id),
        database_id: TEST_DB_ID,
        namespace_id: TEST_NS_ID,
        base_version,
        procedures,
    }
}

pub(crate) fn bound_definition(source: String) -> SrplProcedureDefinition {
    let mut def = SrplProcedureDefinition::from_source(source);
    def.parse().expect("source should parse");
    def.bind_and_lower().expect("source should bind and lower");
    def
}

pub(crate) fn catalog_definition_from_source(
    source: String,
    object_id: u64,
    procedure_id: u64,
    catalog_version: CatalogVersion,
) -> CatalogDefinition {
    bound_definition(source)
        .to_catalog_procedure_def(
            CatalogObjectId::new(object_id),
            ProcedureId::new(procedure_id),
            catalog_version,
        )
        .expect("source should materialize as a catalog Procedure definition")
}
