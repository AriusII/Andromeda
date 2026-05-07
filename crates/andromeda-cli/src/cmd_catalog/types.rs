pub(super) const CATALOG_PREVIEW_MODE: &str = "contract_preview/mock_ephemeral";
pub(super) const CATALOG_PREVIEW_RUNTIME: &str = "mock_ephemeral";
pub(super) const CATALOG_PREVIEW_MESSAGE: &str =
    "mock catalog output: no CatalogServerRuntime durable source was queried";

#[derive(Debug, Clone)]
pub(super) struct ProcedureMetadata {
    pub(super) procedure_id: u64,
    pub(super) name: String,
    pub(super) namespace: Option<String>,
    pub(super) contract_hash: String,
    pub(super) catalog_version: u64,
}

#[derive(Debug, Clone)]
pub(super) struct ProcedureContractInfo {
    pub(super) procedure_id: u64,
    pub(super) name: String,
    pub(super) contract_hash: String,
    pub(super) input_columns: Vec<ColumnInfo>,
    pub(super) output_columns: Vec<ColumnInfo>,
    pub(super) isolation_level: String,
    pub(super) access_mode: String,
}

#[derive(Debug, Clone)]
pub(super) struct ColumnInfo {
    pub(super) name: String,
    pub(super) column_type: String,
    pub(super) nullable: bool,
}

#[derive(Debug, Clone)]
pub(super) struct CacheInvalidationOutcome {
    pub(super) success: bool,
    pub(super) entries_cleared: usize,
    pub(super) cache_invalidated: bool,
    pub(super) message: String,
}

#[derive(Debug, Clone)]
pub(super) struct ProcedureManifestInfo {
    pub(super) procedure_id: u64,
    pub(super) qualified_name: String,
    pub(super) catalog_version: u64,
    pub(super) contract_hash: String,
    pub(super) input_columns: Vec<ColumnInfo>,
    pub(super) output_columns: Vec<ColumnInfo>,
    pub(super) is_mutable: bool,
    pub(super) min_compatible_version: u64,
}
