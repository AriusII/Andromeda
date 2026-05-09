mod gateway_projection;
mod permission;
mod policy_version;
mod procedure_manifest;
mod protocol_layout;
mod result_stream;

#[cfg(test)]
mod tests;

pub use gateway_projection::{
    ProcedureGatewayColumnDescriptor, ProcedureGatewayExecuteRequest, ProcedureGatewayManifest,
    ProcedureGatewayProtocolLayout, ProcedureGatewayRequiredPermission,
    ProcedureGatewayResultStreamDescriptor, required_execute_permission,
    validate_procedure_gateway_manifest_permissions,
};
pub use permission::RequiredPermission;
pub use policy_version::ManifestPolicyVersion;
pub use procedure_manifest::{ProcedureManifest, ProcedureManifestBinding};
pub use protocol_layout::ProtocolLayout;
pub use result_stream::{ResultCardinality, ResultStreamDescriptor, RowCountRequirement};
