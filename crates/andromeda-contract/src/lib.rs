#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Contract Model

Canonical contract and catalog-object descriptor surface shared by catalog,
SRPL, execution, and protocol-facing crates.

This crate owns catalog object descriptors and reexports the split Procedure
contract and StructuredObject contract crates. It does not own catalog storage,
DefinitionBatch application, WAL codecs, RPC runtime behavior, or
StructuredObject wire payloads.
"#]

mod dependencies;
mod objects;

pub use andromeda_procedure_contract::{
    AccessMode, COMPLETION_ENVELOPE_CONTRACT_VERSION, CatalogObjectRef, CompatibilityPolicy,
    CompletionEnvelopeVersion, CompletionProtocolVersion, CompletionTerminalCode,
    ContractCompatibilityDiagnostic, IsolationPolicy, ManifestPolicyVersion, MultiResultPolicy,
    ObjectKind, PolicyVersion, ProcedureContract, ProcedureContractBinding,
    ProcedureContractCandidate, ProcedureContractRef, ProcedureErrorPolicy,
    ProcedureGatewayColumnDescriptor, ProcedureGatewayExecuteRequest, ProcedureGatewayManifest,
    ProcedureGatewayProtocolLayout, ProcedureGatewayRequiredPermission,
    ProcedureGatewayResultStreamDescriptor, ProcedureManifest, ProcedureManifestBinding,
    ProtocolLayout, ProtocolLayoutRef, QualifiedName, RPC_COMPLETION_STATUS_TERMINAL_CODES,
    RequiredPermission, ResultCardinality, ResultMetadataPolicy, ResultRowCountSummary,
    ResultStreamCardinality, ResultStreamContract, ResultStreamDescriptor,
    RowCountRequirement as ProcedureRowCountRequirement, RpcCompletion, RpcCompletionStatus,
    StatsVersion, TRANSACTION_OUTCOME_TERMINAL_CODES, TransactionOutcome, TransactionPolicy,
    diagnose_procedure_contract_compatibility, required_execute_permission,
    validate_procedure_gateway_manifest_permissions,
};
pub use andromeda_structured_object::{
    RowCountPolicy, RowCountRequirement, StructuredObjectHeader, StructuredObjectLayout,
    compute_structured_object_shape_hash, encode_column_descriptors_shape_material,
    encode_structured_object_shape_material, structured_object_shape_hash_compatible,
    validate_structured_object_shape,
};
pub use dependencies::{CatalogDependency, CatalogDependencyKind};
pub use objects::{
    CatalogBindingKind, CatalogDefinition, CatalogObjectBinding, EnumDefinition, EnumVariant,
    StructuredObjectDefinition, TableDefinition,
};
