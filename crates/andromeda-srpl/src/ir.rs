use andromeda_catalog::{CompatibilityPolicy, QualifiedName, TransactionPolicy};
use andromeda_core::{CatalogObjectId, CatalogVersion, ColumnDescriptor, ProcedureId};

use crate::Cardinality;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplProcedureIr {
    pub name: QualifiedName,
    pub inputs: Vec<ColumnDescriptor>,
    pub result_streams: Vec<SrplResultStreamIr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplResultStreamIr {
    pub name: String,
    pub cardinality: Cardinality,
    pub columns: Vec<ColumnDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplProcedureContractMetadata {
    pub object_id: CatalogObjectId,
    pub procedure_id: ProcedureId,
    pub catalog_version: CatalogVersion,
    pub structured_inputs: Vec<QualifiedName>,
    pub required_permissions: Vec<String>,
    pub transaction_policy: TransactionPolicy,
    pub compatibility_policy: CompatibilityPolicy,
}
