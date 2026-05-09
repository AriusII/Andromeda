use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

/// Canonical Procedure manifest projection used at RPC gateway boundaries.
///
/// The type is owned by the Procedure contract crate so transport crates can
/// validate resolved Procedure identity without becoming the source of truth
/// for catalog/procedure manifest shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureGatewayManifest {
    pub procedure_id: ProcedureId,
    pub procedure_name: String,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
    pub protocol_layout: ProcedureGatewayProtocolLayout,
    pub result_streams: Vec<ProcedureGatewayResultStreamDescriptor>,
    pub stats_version: u64,
    pub policy_version: ContractHash,
    pub required_permissions: Vec<ProcedureGatewayRequiredPermission>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureGatewayProtocolLayout {
    pub descriptor_set_hash: ContractHash,
    pub frame_envelope_hash: ContractHash,
    pub protocol_package: String,
    pub contract_package: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureGatewayRequiredPermission {
    pub id: String,
    pub family: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureGatewayResultStreamDescriptor {
    pub stream_name: String,
    pub columns: Vec<ProcedureGatewayColumnDescriptor>,
    pub cardinality: i32,
    pub row_count_requirement: i32,
    pub row_count_exact: Option<u64>,
    pub row_count_max: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureGatewayColumnDescriptor {
    pub name: String,
    pub ordinal: u32,
    pub type_name: String,
}
