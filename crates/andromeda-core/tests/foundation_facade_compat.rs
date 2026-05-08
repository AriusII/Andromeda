use andromeda_core::digest::{Sha256 as CoreSha256, sha256 as core_sha256};
use andromeda_core::{
    AndromedaError as CoreAndromedaError, AndromedaErrorKind as CoreAndromedaErrorKind,
    AndromedaResult as CoreAndromedaResult, CatalogObjectId as CoreCatalogObjectId,
    CatalogVersion as CoreCatalogVersion, Clock as CoreClock,
    ColumnDescriptor as CoreColumnDescriptor, ContractHash as CoreContractHash,
    CpuCapabilityClass as CoreCpuCapabilityClass, CpuProfile as CoreCpuProfile,
    DatabaseId as CoreDatabaseId, DecimalType as CoreDecimalType,
    EngineTimestamp as CoreEngineTimestamp, FloatMode as CoreFloatMode, FloatType as CoreFloatType,
    GpuExecutionPolicy as CoreGpuExecutionPolicy, GpuProfile as CoreGpuProfile,
    HardwareArchitecture as CoreHardwareArchitecture, HardwareProfile as CoreHardwareProfile,
    InvocationId as CoreInvocationId, ManualClock as CoreManualClock,
    NamespaceId as CoreNamespaceId, PipelineClass as CorePipelineClass,
    ProcedureId as CoreProcedureId, RamProfile as CoreRamProfile,
    RamSectionBudget as CoreRamSectionBudget, RamSectionRole as CoreRamSectionRole,
    RequestId as CoreRequestId, ResourceBudget as CoreResourceBudget, ScalarType as CoreScalarType,
    SessionId as CoreSessionId, SystemClock as CoreSystemClock, TextEncoding as CoreTextEncoding,
    TextType as CoreTextType, TimestampType as CoreTimestampType,
    TransactionId as CoreTransactionId, TypeDescriptor as CoreTypeDescriptor,
};
use andromeda_digest::{Sha256 as FoundationSha256, sha256 as foundation_sha256};
use andromeda_error::{
    AndromedaError as FoundationAndromedaError, AndromedaErrorKind as FoundationAndromedaErrorKind,
    AndromedaResult as FoundationAndromedaResult,
};
use andromeda_hardware::{
    CpuCapabilityClass as FoundationCpuCapabilityClass, CpuProfile as FoundationCpuProfile,
    GpuExecutionPolicy as FoundationGpuExecutionPolicy, GpuProfile as FoundationGpuProfile,
    HardwareArchitecture as FoundationHardwareArchitecture,
    HardwareProfile as FoundationHardwareProfile, PipelineClass as FoundationPipelineClass,
    RamProfile as FoundationRamProfile, RamSectionBudget as FoundationRamSectionBudget,
    RamSectionRole as FoundationRamSectionRole, ResourceBudget as FoundationResourceBudget,
};
use andromeda_time::{
    Clock as FoundationClock, EngineTimestamp as FoundationEngineTimestamp,
    ManualClock as FoundationManualClock, SystemClock as FoundationSystemClock,
};
use andromeda_types::{
    CatalogObjectId as FoundationCatalogObjectId, CatalogVersion as FoundationCatalogVersion,
    ColumnDescriptor as FoundationColumnDescriptor, ContractHash as FoundationContractHash,
    DatabaseId as FoundationDatabaseId, DecimalType as FoundationDecimalType,
    FloatMode as FoundationFloatMode, FloatType as FoundationFloatType,
    InvocationId as FoundationInvocationId, NamespaceId as FoundationNamespaceId,
    ProcedureId as FoundationProcedureId, RequestId as FoundationRequestId,
    ScalarType as FoundationScalarType, SessionId as FoundationSessionId,
    TextEncoding as FoundationTextEncoding, TextType as FoundationTextType,
    TimestampType as FoundationTimestampType, TransactionId as FoundationTransactionId,
    TypeDescriptor as FoundationTypeDescriptor,
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

    let _: Option<FoundationColumnDescriptor> = Option::<CoreColumnDescriptor>::None;
    let _: Option<FoundationDecimalType> = Option::<CoreDecimalType>::None;
    let _: Option<FoundationFloatMode> = Option::<CoreFloatMode>::None;
    let _: Option<FoundationFloatType> = Option::<CoreFloatType>::None;
    let _: Option<FoundationTypeDescriptor> = Option::<CoreTypeDescriptor>::None;
    let _: Option<FoundationScalarType> = Option::<CoreScalarType>::None;
    let _: Option<FoundationTextEncoding> = Option::<CoreTextEncoding>::None;
    let _: Option<FoundationTextType> = Option::<CoreTextType>::None;
    let _: Option<FoundationTimestampType> = Option::<CoreTimestampType>::None;
    let _: Option<FoundationEngineTimestamp> = Option::<CoreEngineTimestamp>::None;
    let _: Option<FoundationManualClock> = Option::<CoreManualClock>::None;
    let _: Option<FoundationSystemClock> = Option::<CoreSystemClock>::None;
    let _: Option<&dyn FoundationClock> = Option::<&dyn CoreClock>::None;

    let _: Option<FoundationSha256> = Option::<CoreSha256>::None;
    let _: fn(&[u8]) -> [u8; 32] = core_sha256;
    let _: fn(&[u8]) -> [u8; 32] = foundation_sha256;

    let _: Option<FoundationCpuCapabilityClass> = Option::<CoreCpuCapabilityClass>::None;
    let _: Option<FoundationCpuProfile> = Option::<CoreCpuProfile>::None;
    let _: Option<FoundationHardwareArchitecture> = Option::<CoreHardwareArchitecture>::None;
    let _: Option<FoundationHardwareProfile> = Option::<CoreHardwareProfile>::None;
    let _: Option<FoundationResourceBudget> = Option::<CoreResourceBudget>::None;
    let _: Option<FoundationGpuExecutionPolicy> = Option::<CoreGpuExecutionPolicy>::None;
    let _: Option<FoundationGpuProfile> = Option::<CoreGpuProfile>::None;
    let _: Option<FoundationPipelineClass> = Option::<CorePipelineClass>::None;
    let _: Option<FoundationRamProfile> = Option::<CoreRamProfile>::None;
    let _: Option<FoundationRamSectionBudget> = Option::<CoreRamSectionBudget>::None;
    let _: Option<FoundationRamSectionRole> = Option::<CoreRamSectionRole>::None;
}

#[test]
fn core_facade_preserves_foundation_behavior() {
    let error = CoreAndromedaError::new(CoreAndromedaErrorKind::Contract, "hash mismatch");
    assert_eq!(error.code(), "AE0002");
    assert_eq!(error.render_diagnostic(), "AE0002 contract: hash mismatch");

    let timestamp = CoreEngineTimestamp::from_unix_millis(0x0102_0304_0506_0708);
    let encoded = timestamp.to_unix_millis_le_bytes();
    assert_eq!(
        CoreEngineTimestamp::try_from_unix_millis_le_slice(&encoded),
        Ok(timestamp)
    );

    let text = CoreTypeDescriptor::required(CoreScalarType::Text(CoreTextType {
        encoding: CoreTextEncoding::Utf8,
        max_length: Some(128),
        collation: Some("unicode:case-sensitive".to_string()),
    }));
    assert!(text.validate().is_ok());

    let gpu = CoreGpuProfile::batch_analytics_only();
    assert_eq!(
        gpu.select_advisory_gpu(CorePipelineClass::BatchAnalytics, true, true),
        Ok(true)
    );
    assert!(
        !CoreGpuExecutionPolicy::OffCriticalPathOnly
            .permits_pipeline(CorePipelineClass::CatalogPublication)
    );
}

#[test]
fn policy_facade_preserves_historical_wildcard_import_path() {
    use andromeda_core::policy::*;

    let _: Option<andromeda_hardware::CpuCapabilityClass> = Option::<CpuCapabilityClass>::None;
    let _: Option<andromeda_hardware::CpuProfile> = Option::<CpuProfile>::None;
    let _: Option<andromeda_hardware::HardwareArchitecture> = Option::<HardwareArchitecture>::None;
    let _: Option<andromeda_hardware::HardwareProfile> = Option::<HardwareProfile>::None;
    let _: Option<andromeda_hardware::ResourceBudget> = Option::<ResourceBudget>::None;
    let _: Option<andromeda_hardware::GpuExecutionPolicy> = Option::<GpuExecutionPolicy>::None;
    let _: Option<andromeda_hardware::GpuProfile> = Option::<GpuProfile>::None;
    let _: Option<andromeda_hardware::PipelineClass> = Option::<PipelineClass>::None;
    let _: Option<andromeda_hardware::RamProfile> = Option::<RamProfile>::None;
    let _: Option<andromeda_hardware::RamSectionBudget> = Option::<RamSectionBudget>::None;
    let _: Option<andromeda_hardware::RamSectionRole> = Option::<RamSectionRole>::None;
}
