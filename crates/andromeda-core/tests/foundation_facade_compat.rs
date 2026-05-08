use andromeda_core::digest::{Sha256 as CoreSha256, sha256 as core_sha256};
use andromeda_core::{
    AndromedaError as CoreAndromedaError, AndromedaErrorKind as CoreAndromedaErrorKind,
    AndromedaResult as CoreAndromedaResult, CatalogObjectId as CoreCatalogObjectId,
    CatalogVersion as CoreCatalogVersion, ContractHash as CoreContractHash,
    DatabaseId as CoreDatabaseId, EngineTimestamp as CoreEngineTimestamp,
    GpuExecutionPolicy as CoreGpuExecutionPolicy, HardwareProfile as CoreHardwareProfile,
    InvocationId as CoreInvocationId, NamespaceId as CoreNamespaceId,
    PipelineClass as CorePipelineClass, ProcedureId as CoreProcedureId, RequestId as CoreRequestId,
    ResourceBudget as CoreResourceBudget, ScalarType as CoreScalarType, SessionId as CoreSessionId,
    TransactionId as CoreTransactionId, TypeDescriptor as CoreTypeDescriptor,
};
use andromeda_digest::{Sha256 as FoundationSha256, sha256 as foundation_sha256};
use andromeda_error::{
    AndromedaError as FoundationAndromedaError, AndromedaErrorKind as FoundationAndromedaErrorKind,
    AndromedaResult as FoundationAndromedaResult,
};
use andromeda_hardware::{
    GpuExecutionPolicy as FoundationGpuExecutionPolicy,
    HardwareProfile as FoundationHardwareProfile, PipelineClass as FoundationPipelineClass,
    ResourceBudget as FoundationResourceBudget,
};
use andromeda_time::EngineTimestamp as FoundationEngineTimestamp;
use andromeda_types::{
    CatalogObjectId as FoundationCatalogObjectId, CatalogVersion as FoundationCatalogVersion,
    ContractHash as FoundationContractHash, DatabaseId as FoundationDatabaseId,
    InvocationId as FoundationInvocationId, NamespaceId as FoundationNamespaceId,
    ProcedureId as FoundationProcedureId, RequestId as FoundationRequestId,
    ScalarType as FoundationScalarType, SessionId as FoundationSessionId,
    TransactionId as FoundationTransactionId, TypeDescriptor as FoundationTypeDescriptor,
};

#[test]
fn core_facade_preserves_direct_foundation_public_imports() {
    let _: Option<FoundationAndromedaError> = Option::<CoreAndromedaError>::None;
    let _: Option<FoundationAndromedaErrorKind> = Option::<CoreAndromedaErrorKind>::None;

    let core_result: CoreAndromedaResult<()> = Ok(());
    let _: FoundationAndromedaResult<()> = core_result;

    let _: Option<FoundationCatalogObjectId> = Option::<CoreCatalogObjectId>::None;
    let _: Option<FoundationCatalogVersion> = Option::<CoreCatalogVersion>::None;
    let _: Option<FoundationContractHash> = Option::<CoreContractHash>::None;
    let _: Option<FoundationDatabaseId> = Option::<CoreDatabaseId>::None;
    let _: Option<FoundationInvocationId> = Option::<CoreInvocationId>::None;
    let _: Option<FoundationNamespaceId> = Option::<CoreNamespaceId>::None;
    let _: Option<FoundationProcedureId> = Option::<CoreProcedureId>::None;
    let _: Option<FoundationRequestId> = Option::<CoreRequestId>::None;
    let _: Option<FoundationSessionId> = Option::<CoreSessionId>::None;
    let _: Option<FoundationTransactionId> = Option::<CoreTransactionId>::None;

    let _: Option<FoundationContractHash> = Option::<CoreContractHash>::None;
    let _: Option<FoundationTypeDescriptor> = Option::<CoreTypeDescriptor>::None;
    let _: Option<FoundationScalarType> = Option::<CoreScalarType>::None;
    let _: Option<FoundationEngineTimestamp> = Option::<CoreEngineTimestamp>::None;

    let _: Option<FoundationSha256> = Option::<CoreSha256>::None;
    let _: fn(&[u8]) -> [u8; 32] = core_sha256;
    let _: fn(&[u8]) -> [u8; 32] = foundation_sha256;

    let _: Option<FoundationHardwareProfile> = Option::<CoreHardwareProfile>::None;
    let _: Option<FoundationResourceBudget> = Option::<CoreResourceBudget>::None;
    let _: Option<FoundationGpuExecutionPolicy> = Option::<CoreGpuExecutionPolicy>::None;
    let _: Option<FoundationPipelineClass> = Option::<CorePipelineClass>::None;
}

#[test]
fn policy_facade_preserves_historical_wildcard_import_path() {
    use andromeda_core::policy::*;

    let _: Option<andromeda_hardware::HardwareProfile> = Option::<HardwareProfile>::None;
    let _: Option<andromeda_hardware::ResourceBudget> = Option::<ResourceBudget>::None;
    let _: Option<andromeda_hardware::GpuExecutionPolicy> = Option::<GpuExecutionPolicy>::None;
    let _: Option<andromeda_hardware::PipelineClass> = Option::<PipelineClass>::None;
}
