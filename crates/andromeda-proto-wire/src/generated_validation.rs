use std::collections::{BTreeMap, BTreeSet};

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_procedure_contract::{
    ResultRowCountSummary, RpcCompletion, RpcCompletionStatus, TransactionOutcome,
};
use andromeda_structured_object::{RowCountPolicy, StructuredObjectHeader, StructuredObjectLayout};
use andromeda_types::{
    CatalogVersion, ColumnDescriptor, ContractHash, RequestId, ScalarType, SessionId,
    TransactionId, TypeDescriptor,
};

use crate::{
    CONTRACT_PACKAGE, FrameEnvelope, GeneratedColumnDescriptor, GeneratedFrameEnvelope,
    GeneratedProtocolVersion, GeneratedResultCompletionPolicy, GeneratedResultRowCountSummary,
    GeneratedResultStreamDescriptor, GeneratedRpcBatch, GeneratedRpcCompletion,
    GeneratedRpcMetadata, PROTOCOL_PACKAGE, PayloadKind, ProtocolVersion,
    StructuredPayloadByteTracker, validate_optional_contract_hash, validate_required_contract_hash,
    validate_result_batch_payload_parts, validate_rpc_execute_argument_value,
    validate_rpc_execute_arguments_len, validate_rpc_response_sequence_len,
};

const MAX_GENERATED_RESULT_STREAMS: usize = 128;
const MAX_GENERATED_RESULT_STREAM_COLUMNS: usize = 256;
const MAX_GENERATED_STRUCTURED_OBJECT_FIELDS: usize = 256;
const MAX_GENERATED_STRUCTURED_OBJECT_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;

pub enum GeneratedCatalogManifestResolutionSelector<'a> {
    ProcedureId(u64),
    ProcedureName(&'a str),
}

pub enum GeneratedInvocationResponsePayload<'a, Metadata, Batch, Completion, Error> {
    Metadata(&'a Metadata),
    Batch(&'a Batch),
    Completion(&'a Completion),
    Error(&'a Error),
}

pub trait GeneratedProtocolVersionView {
    fn major(&self) -> u32;
    fn minor(&self) -> u32;
}

pub trait GeneratedFrameEnvelopeView {
    type ProtocolVersion: GeneratedProtocolVersionView;

    fn protocol_version(&self) -> Option<&Self::ProtocolVersion>;
    fn contract_hash(&self) -> &[u8];
    fn catalog_version(&self) -> u64;
    fn request_id(&self) -> u64;
    fn session_id(&self) -> u64;
    fn tx_id(&self) -> Option<u64>;
    fn payload_kind(&self) -> i32;
    fn payload(&self) -> &[u8];
}

pub trait GeneratedColumnDescriptorView {
    fn name(&self) -> &str;
    fn ordinal(&self) -> u32;
    fn type_name(&self) -> &str;
}

pub trait GeneratedResultStreamDescriptorView {
    type Column: GeneratedColumnDescriptorView;

    fn stream_name(&self) -> &str;
    fn columns(&self) -> &[Self::Column];
    fn cardinality(&self) -> i32;
    fn row_count_requirement(&self) -> i32;
    fn row_count_exact(&self) -> Option<u64>;
    fn row_count_max(&self) -> Option<u64>;
}

pub trait GeneratedRequiredPermissionView {
    fn id(&self) -> &str;
    fn family(&self) -> &str;
}

pub trait GeneratedProtocolLayoutView {
    fn descriptor_set_hash(&self) -> &[u8];
    fn frame_envelope_hash(&self) -> &[u8];
    fn protocol_package(&self) -> &str;
    fn contract_package(&self) -> &str;
}

pub trait GeneratedProcedureManifestView {
    type ProtocolLayout: GeneratedProtocolLayoutView;
    type ResultStream: GeneratedResultStreamDescriptorView;
    type RequiredPermission: GeneratedRequiredPermissionView;

    fn procedure_id(&self) -> u64;
    fn procedure_name(&self) -> &str;
    fn contract_hash(&self) -> &[u8];
    fn catalog_version(&self) -> u64;
    fn protocol_layout(&self) -> Option<&Self::ProtocolLayout>;
    fn result_streams(&self) -> &[Self::ResultStream];
    fn policy_version(&self) -> &[u8];
    fn required_permissions(&self) -> &[Self::RequiredPermission];
    fn stats_version(&self) -> Option<u64>;
}

pub trait GeneratedCatalogManifestResolutionRequestView {
    fn protocol_major(&self) -> u32;
    fn protocol_minor(&self) -> u32;
    fn request_id(&self) -> u64;
    fn selector(&self) -> Option<GeneratedCatalogManifestResolutionSelector<'_>>;
    fn expected_contract_hash(&self) -> Option<&[u8]>;
    fn expected_catalog_version(&self) -> Option<u64>;
}

pub trait GeneratedCatalogManifestResolutionResponseView {
    type Manifest: GeneratedProcedureManifestView;

    fn protocol_major(&self) -> u32;
    fn protocol_minor(&self) -> u32;
    fn request_id(&self) -> u64;
    fn status(&self) -> i32;
    fn manifest(&self) -> Option<&Self::Manifest>;
    fn resolved_contract_hash(&self) -> Option<&[u8]>;
    fn resolved_catalog_version(&self) -> Option<u64>;
    fn current_catalog_version(&self) -> Option<u64>;
}

pub trait GeneratedResultCompletionPolicyView {
    fn completion_shape(&self) -> i32;
}

pub trait GeneratedRpcMetadataView {
    type ResultStream: GeneratedResultStreamDescriptorView;
    type CompletionPolicy: GeneratedResultCompletionPolicyView;

    fn result_streams(&self) -> &[Self::ResultStream];
    fn completion_policy(&self) -> Option<&Self::CompletionPolicy>;
}

pub trait GeneratedRpcBatchView {
    fn result_name(&self) -> &str;
    fn batch_index(&self) -> u64;
    fn rows_emitted(&self) -> u64;
    fn structured_payload(&self) -> &[u8];
    fn row_count_exact(&self) -> Option<u64>;
    fn terminal_batch(&self) -> bool;
}

pub trait GeneratedResultRowCountSummaryView {
    fn result_name(&self) -> &str;
    fn rows_emitted(&self) -> u64;
    fn row_count_exact(&self) -> Option<u64>;
}

pub trait GeneratedRpcCompletionView {
    type ResultRowCountSummary: GeneratedResultRowCountSummaryView;

    fn status(&self) -> i32;
    fn rows_affected(&self) -> Option<u64>;
    fn tx_id(&self) -> Option<u64>;
    fn request_id(&self) -> Option<u64>;
    fn session_id(&self) -> Option<u64>;
    fn trace_id(&self) -> Option<&str>;
    fn transaction_outcome(&self) -> i32;
    fn durable_lsn(&self) -> Option<u64>;
    fn result_row_counts(&self) -> &[Self::ResultRowCountSummary];
}

pub trait GeneratedBackpressureMetadataView {
    fn retry_after_ms(&self) -> Option<u64>;
    fn capacity_percent(&self) -> Option<u32>;
}

pub trait GeneratedErrorEnvelopeView {
    type Backpressure: GeneratedBackpressureMetadataView;

    fn request_id(&self) -> Option<u64>;
    fn session_id(&self) -> Option<u64>;
    fn trace_id(&self) -> Option<&str>;
    fn family(&self) -> i32;
    fn code(&self) -> &str;
    fn message(&self) -> &str;
    fn transaction_effect(&self) -> i32;
    fn retry_disposition(&self) -> i32;
    fn retry_after_ms(&self) -> Option<u64>;
    fn backpressure(&self) -> Option<&Self::Backpressure>;
}

pub trait GeneratedRpcExecuteArgumentView {
    fn name(&self) -> &str;
    fn type_name(&self) -> &str;
    fn value(&self) -> &[u8];
}

pub trait GeneratedRpcExecuteRequestBudgetView {
    fn cpu_micros(&self) -> Option<u64>;
    fn memory_bytes(&self) -> Option<u64>;
    fn io_bytes(&self) -> Option<u64>;
    fn priority_class(&self) -> Option<u32>;
}

pub trait GeneratedRpcExecuteRequestView {
    type Argument: GeneratedRpcExecuteArgumentView;
    type Budget: GeneratedRpcExecuteRequestBudgetView;

    fn procedure_name(&self) -> &str;
    fn expected_contract_hash(&self) -> &[u8];
    fn expected_catalog_version(&self) -> u64;
    fn surface_scope(&self) -> &str;
    fn arguments(&self) -> &[Self::Argument];
    fn budget(&self) -> Option<&Self::Budget>;
    fn expected_stats_version(&self) -> Option<u64>;
}

pub trait GeneratedInvocationCorrelationView {
    fn request_id(&self) -> Option<u64>;
    fn session_id(&self) -> Option<u64>;
    fn trace_id(&self) -> Option<&str>;
    fn contract_hash(&self) -> Option<&[u8]>;
    fn catalog_version(&self) -> Option<u64>;
    fn invocation_id(&self) -> Option<u64>;
    fn stats_version(&self) -> Option<u64>;
    fn expected_policy_version(&self) -> Option<u64>;
}

pub trait GeneratedInvocationRequestView {
    type Correlation: GeneratedInvocationCorrelationView;
    type ExecuteRequest: GeneratedRpcExecuteRequestView;

    fn correlation(&self) -> Option<&Self::Correlation>;
    fn execute_request(&self) -> Option<&Self::ExecuteRequest>;
}

pub type GeneratedInvocationResponsePayloadFor<'a, T> = GeneratedInvocationResponsePayload<
    'a,
    <T as GeneratedInvocationResponseView>::Metadata,
    <T as GeneratedInvocationResponseView>::Batch,
    <T as GeneratedInvocationResponseView>::Completion,
    <T as GeneratedInvocationResponseView>::Error,
>;

pub trait GeneratedInvocationResponseView {
    type Correlation: GeneratedInvocationCorrelationView;
    type Metadata: GeneratedRpcMetadataView;
    type Batch: GeneratedRpcBatchView;
    type Completion: GeneratedRpcCompletionView;
    type Error: GeneratedErrorEnvelopeView;

    fn correlation(&self) -> Option<&Self::Correlation>;
    fn response_index(&self) -> Option<u64>;
    fn payload(&self) -> Option<GeneratedInvocationResponsePayloadFor<'_, Self>>;
}

pub trait GeneratedStructuredObjectHeaderView {
    type Column: GeneratedColumnDescriptorView;

    fn name(&self) -> &str;
    fn contract_hash(&self) -> &[u8];
    fn descriptor_hash(&self) -> &[u8];
    fn row_count_exact(&self) -> u64;
    fn column_count(&self) -> u32;
    fn layout(&self) -> i32;
    fn payload_length(&self) -> u64;
    fn payload_checksum(&self) -> Option<u64>;
    fn max_payload_length(&self) -> Option<u64>;
    fn fields(&self) -> &[Self::Column];
    fn row_count_policy(&self) -> i32;
}

pub fn validate_generated_protocol_version(major: u32, minor: u32) -> AndromedaResult<()> {
    ProtocolVersion { major, minor }.validate()
}

pub fn project_generated_frame_envelope<T>(envelope: &T) -> AndromedaResult<FrameEnvelope>
where
    T: GeneratedFrameEnvelopeView,
{
    let Some(version) = envelope.protocol_version() else {
        return protocol_error("generated frame envelope requires protocol_version");
    };

    let payload_kind = project_payload_kind(envelope.payload_kind())?;
    let contract_hash = project_contract_hash_for_payload_kind(
        "generated frame envelope contract_hash",
        payload_kind,
        envelope.contract_hash(),
    )?;

    FrameEnvelope {
        protocol_version: ProtocolVersion {
            major: version.major(),
            minor: version.minor(),
        },
        contract_hash,
        catalog_version: CatalogVersion::new(envelope.catalog_version()),
        request_id: RequestId::new(envelope.request_id()),
        session_id: SessionId::new(envelope.session_id()),
        tx_id: envelope.tx_id().map(TransactionId::new),
        payload_kind,
        payload: envelope.payload().to_vec(),
    }
    .validated()
}

pub fn validate_generated_frame_envelope<T>(envelope: &T) -> AndromedaResult<()>
where
    T: GeneratedFrameEnvelopeView,
{
    project_generated_frame_envelope(envelope).map(|_| ())
}

pub fn validate_generated_rpc_metadata<T>(metadata: &T) -> AndromedaResult<()>
where
    T: GeneratedRpcMetadataView,
{
    validate_generated_result_streams(metadata.result_streams())?;

    let Some(policy) = metadata.completion_policy() else {
        return contract_error("generated RPC metadata requires completion_policy");
    };

    validate_result_completion_policy(policy)
}

pub fn validate_generated_rpc_batch<T>(batch: &T) -> AndromedaResult<()>
where
    T: GeneratedRpcBatchView,
{
    if batch.result_name().trim().is_empty() {
        return contract_error("generated RPC batch result_name must be non-empty");
    }

    validate_result_batch_payload_parts(
        batch.structured_payload(),
        batch.rows_emitted(),
        batch.row_count_exact(),
    )
}

pub fn validate_generated_rpc_completion<T>(completion: &T) -> AndromedaResult<()>
where
    T: GeneratedRpcCompletionView,
{
    let status = validate_completion_status(completion.status())?;
    let transaction_outcome = validate_transaction_outcome(completion.transaction_outcome())?;
    let result_row_counts = completion
        .result_row_counts()
        .iter()
        .map(|summary| ResultRowCountSummary {
            result_name: summary.result_name().to_string(),
            rows_emitted: summary.rows_emitted(),
            row_count_exact: summary.row_count_exact(),
        })
        .collect::<Vec<_>>();

    RpcCompletion {
        request_id: completion.request_id().map(RequestId::new),
        session_id: completion.session_id().map(SessionId::new),
        trace_id: completion.trace_id().map(str::to_string),
        status,
        transaction_outcome,
        rows_affected: completion.rows_affected(),
        result_row_counts,
        tx_id: completion.tx_id().map(TransactionId::new),
        durable_lsn: completion.durable_lsn(),
    }
    .validate()
}

pub fn validate_generated_error_envelope<T>(error: &T) -> AndromedaResult<()>
where
    T: GeneratedErrorEnvelopeView,
{
    validate_error_family(error.family())?;
    validate_transaction_effect(error.transaction_effect())?;
    let retry_disposition = validate_retry_disposition(error.retry_disposition())?;

    if error.code().trim().is_empty() {
        return contract_error("error envelope code must not be empty");
    }

    if error.message().trim().is_empty() {
        return contract_error("error envelope message must not be empty");
    }

    if matches!(error.trace_id(), Some(trace_id) if trace_id.trim().is_empty()) {
        return contract_error("error trace correlation id must not be empty when present");
    }

    if let Some(backpressure) = error.backpressure() {
        validate_backpressure_metadata(backpressure)?;
    }

    if retry_disposition == GeneratedRetryDispositionCode::RetryAfter
        && error.retry_after_ms().is_none()
        && error
            .backpressure()
            .and_then(GeneratedBackpressureMetadataView::retry_after_ms)
            .is_none()
    {
        return resource_error("retry-after error must include retry delay metadata");
    }

    if retry_disposition == GeneratedRetryDispositionCode::Backpressure
        && error.backpressure().is_none()
    {
        return resource_error("backpressure error must include backpressure metadata");
    }

    Ok(())
}

pub fn validate_generated_rpc_execute_request<T>(request: &T) -> AndromedaResult<()>
where
    T: GeneratedRpcExecuteRequestView,
{
    if request.procedure_name().trim().is_empty() {
        return contract_error("generated RPC execute request procedure_name must be non-empty");
    }

    validate_required_contract_hash(
        "generated RPC execute request expected_contract_hash",
        request.expected_contract_hash(),
    )?;

    validate_optional_catalog_version(
        "generated RPC execute request expected_catalog_version",
        Some(request.expected_catalog_version()),
    )?;

    match request.expected_stats_version() {
        Some(value) if value != 0 => {}
        Some(_) => {
            return contract_error(
                "generated RPC execute request expected_stats_version must be nonzero",
            );
        }
        None => {
            return contract_error("generated RPC execute request requires expected_stats_version");
        }
    }

    if request.surface_scope().trim().is_empty() {
        return contract_error("generated RPC execute request surface_scope must be non-empty");
    }

    validate_rpc_execute_arguments(request.arguments())?;
    validate_rpc_execute_budget(request.budget())?;

    Ok(())
}

pub fn validate_generated_invocation_request<T>(request: &T) -> AndromedaResult<()>
where
    T: GeneratedInvocationRequestView,
{
    let Some(correlation) = request.correlation() else {
        return contract_error("generated invocation request requires correlation");
    };
    let Some(execute_request) = request.execute_request() else {
        return contract_error("generated invocation request requires execute_request");
    };

    validate_generated_rpc_execute_request(execute_request)?;
    validate_required_invocation_correlation(
        "generated invocation request correlation",
        correlation,
    )?;

    let Some(correlation_contract_hash) = correlation.contract_hash() else {
        return contract_error("generated invocation request correlation requires contract_hash");
    };
    if correlation_contract_hash != execute_request.expected_contract_hash() {
        return contract_error(
            "generated invocation request correlation contract_hash must match execute request",
        );
    }

    if correlation.catalog_version() != Some(execute_request.expected_catalog_version()) {
        return contract_error(
            "generated invocation request correlation catalog_version must match execute request",
        );
    }

    if correlation.stats_version() != execute_request.expected_stats_version() {
        return contract_error(
            "generated invocation request correlation stats_version must match execute request",
        );
    }

    validate_invocation_policy_version_required(
        "generated invocation request security context",
        correlation.expected_policy_version(),
    )?;

    Ok(())
}

pub fn validate_generated_invocation_response<T>(response: &T) -> AndromedaResult<()>
where
    T: GeneratedInvocationResponseView,
{
    let Some(correlation) = response.correlation() else {
        return contract_error("generated invocation response requires correlation");
    };
    validate_required_invocation_correlation(
        "generated invocation response correlation",
        correlation,
    )?;

    match response.payload() {
        Some(GeneratedInvocationResponsePayload::Metadata(metadata)) => {
            validate_generated_rpc_metadata(metadata)
        }
        Some(GeneratedInvocationResponsePayload::Batch(batch)) => {
            validate_generated_rpc_batch(batch)
        }
        Some(GeneratedInvocationResponsePayload::Completion(completion)) => {
            validate_generated_rpc_completion(completion)?;
            validate_response_correlation_ids(
                "generated invocation completion",
                correlation,
                completion.request_id(),
                completion.session_id(),
            )
        }
        Some(GeneratedInvocationResponsePayload::Error(error)) => {
            validate_generated_error_envelope(error)?;
            validate_response_correlation_ids(
                "generated invocation error",
                correlation,
                error.request_id(),
                error.session_id(),
            )
        }
        None => contract_error("generated invocation response requires a typed response payload"),
    }
}

pub fn validate_generated_invocation_response_sequence<T>(responses: &[T]) -> AndromedaResult<()>
where
    T: GeneratedInvocationResponseView,
{
    if responses.is_empty() {
        return contract_error("generated invocation response sequence must be non-empty");
    }
    validate_rpc_response_sequence_len(responses.len())?;

    let mut state = InvocationResponseSequenceState::default();
    for (ordinal, response) in responses.iter().enumerate() {
        validate_generated_invocation_response(response)?;
        state.observe(ordinal, response)?;
    }

    state.finish()
}

pub fn project_generated_structured_object_header<T>(
    header: &T,
) -> AndromedaResult<StructuredObjectHeader>
where
    T: GeneratedStructuredObjectHeaderView,
{
    validate_required_contract_hash(
        "generated StructuredObjectHeader contract_hash",
        header.contract_hash(),
    )?;
    validate_required_contract_hash(
        "generated StructuredObjectHeader descriptor_hash",
        header.descriptor_hash(),
    )?;
    validate_generated_structured_object_payload_limits(header)?;

    let fields = project_generated_structured_object_fields(header.fields())?;
    let typed = StructuredObjectHeader {
        name: header.name().to_string(),
        contract_hash: ContractHash::from_slice(header.contract_hash())?,
        descriptor_hash: ContractHash::from_slice(header.descriptor_hash())?,
        fields,
        column_count: header.column_count(),
        layout: project_generated_structured_object_layout(header.layout())?,
        row_count_policy: project_generated_structured_object_row_count_policy(
            header.row_count_policy(),
        )?,
        row_count_exact: project_generated_structured_object_row_count_exact(
            header.row_count_exact(),
        )?,
        payload_length: header.payload_length(),
        payload_checksum: header.payload_checksum(),
        max_payload_length: header.max_payload_length(),
    };

    typed.validate()?;
    Ok(typed)
}

pub fn validate_generated_structured_object_header<T>(header: &T) -> AndromedaResult<()>
where
    T: GeneratedStructuredObjectHeaderView,
{
    project_generated_structured_object_header(header).map(|_| ())
}

pub fn validate_catalog_procedure_manifest_resolution_request<T>(request: &T) -> AndromedaResult<()>
where
    T: GeneratedCatalogManifestResolutionRequestView,
{
    validate_generated_protocol_version(request.protocol_major(), request.protocol_minor())?;

    if request.request_id() == 0 {
        return protocol_error("catalog manifest resolution request_id must be nonzero");
    }

    match request.selector() {
        Some(GeneratedCatalogManifestResolutionSelector::ProcedureId(procedure_id))
            if procedure_id != 0 => {}
        Some(GeneratedCatalogManifestResolutionSelector::ProcedureName(procedure_name))
            if !procedure_name.trim().is_empty() => {}
        Some(_) => {
            return contract_error(
                "catalog manifest resolution selector must be nonzero/non-empty",
            );
        }
        None => {
            return contract_error("catalog manifest resolution request requires a selector");
        }
    }

    validate_optional_contract_hash(
        "catalog manifest resolution expected_contract_hash",
        request.expected_contract_hash(),
    )?;
    validate_optional_catalog_version(
        "catalog manifest resolution expected_catalog_version",
        request.expected_catalog_version(),
    )?;

    Ok(())
}

pub fn validate_catalog_procedure_manifest_resolution_response<T>(
    response: &T,
) -> AndromedaResult<()>
where
    T: GeneratedCatalogManifestResolutionResponseView,
{
    validate_generated_protocol_version(response.protocol_major(), response.protocol_minor())?;

    if response.request_id() == 0 {
        return protocol_error("catalog manifest resolution response request_id must be nonzero");
    }

    let status = validate_resolution_status(response.status())?;
    validate_optional_contract_hash(
        "catalog manifest resolution resolved_contract_hash",
        response.resolved_contract_hash(),
    )?;
    validate_optional_catalog_version(
        "catalog manifest resolution resolved_catalog_version",
        response.resolved_catalog_version(),
    )?;
    validate_optional_catalog_version(
        "catalog manifest resolution current_catalog_version",
        response.current_catalog_version(),
    )?;

    if status == ResolutionStatusCode::Resolved {
        let Some(manifest) = response.manifest() else {
            return contract_error("resolved catalog manifest response requires a manifest");
        };
        validate_generated_procedure_manifest(manifest)?;

        if response.resolved_contract_hash() != Some(manifest.contract_hash()) {
            return contract_error("resolved contract hash must match manifest contract_hash");
        }

        if response.resolved_catalog_version() != Some(manifest.catalog_version()) {
            return contract_error("resolved catalog version must match manifest catalog_version");
        }
    } else {
        validate_unresolved_response_fields(response)?;
    }

    Ok(())
}

pub fn validate_generated_procedure_manifest<T>(manifest: &T) -> AndromedaResult<()>
where
    T: GeneratedProcedureManifestView,
{
    if manifest.procedure_id() == 0 {
        return contract_error("resolved procedure manifest procedure_id must be nonzero");
    }

    if manifest.procedure_name().trim().is_empty() {
        return contract_error("resolved procedure manifest procedure_name must be non-empty");
    }

    validate_required_contract_hash(
        "resolved procedure manifest contract_hash",
        manifest.contract_hash(),
    )?;
    validate_optional_catalog_version(
        "resolved procedure manifest catalog_version",
        Some(manifest.catalog_version()),
    )?;
    validate_required_stats_version(
        "resolved procedure manifest stats_version",
        manifest.stats_version(),
    )?;
    validate_required_contract_hash(
        "resolved procedure manifest policy_version",
        manifest.policy_version(),
    )?;

    let Some(protocol_layout) = manifest.protocol_layout() else {
        return contract_error("resolved procedure manifest requires protocol_layout");
    };
    let contract_hash = manifest.contract_hash();
    let policy_version = manifest.policy_version();
    let descriptor_set_hash = protocol_layout.descriptor_set_hash();
    let frame_envelope_hash = protocol_layout.frame_envelope_hash();
    validate_required_contract_hash(
        "resolved procedure manifest descriptor_set_hash",
        descriptor_set_hash,
    )?;
    validate_required_contract_hash(
        "resolved procedure manifest frame_envelope_hash",
        frame_envelope_hash,
    )?;
    if protocol_layout.protocol_package() != PROTOCOL_PACKAGE {
        return contract_error("resolved procedure manifest protocol_package mismatch");
    }
    if protocol_layout.contract_package() != CONTRACT_PACKAGE {
        return contract_error("resolved procedure manifest contract_package mismatch");
    }
    if descriptor_set_hash == frame_envelope_hash {
        return contract_error(
            "resolved procedure manifest descriptor and frame envelope hashes must be distinct",
        );
    }
    if contract_hash == descriptor_set_hash || contract_hash == frame_envelope_hash {
        return contract_error(
            "resolved procedure manifest contract_hash must be distinct from protocol layout hashes",
        );
    }
    if policy_version == contract_hash
        || policy_version == descriptor_set_hash
        || policy_version == frame_envelope_hash
    {
        return contract_error(
            "resolved procedure manifest policy_version must be distinct from contract and protocol layout hashes",
        );
    }

    validate_generated_result_streams(manifest.result_streams())?;
    validate_generated_required_permissions(manifest.required_permissions())?;

    Ok(())
}

pub fn validate_generated_result_streams<T>(descriptors: &[T]) -> AndromedaResult<()>
where
    T: GeneratedResultStreamDescriptorView,
{
    if descriptors.len() > MAX_GENERATED_RESULT_STREAMS {
        return contract_error(format!(
            "resolved procedure manifest result streams exceed bounded limit of {MAX_GENERATED_RESULT_STREAMS}"
        ));
    }

    let mut seen_streams = BTreeSet::new();
    for descriptor in descriptors {
        validate_generated_result_stream_descriptor(descriptor)?;
        if !seen_streams.insert(descriptor.stream_name().to_string()) {
            return contract_error(
                "resolved procedure manifest result stream names must be unique",
            );
        }
    }

    Ok(())
}

fn validate_generated_result_stream_descriptor<T>(descriptor: &T) -> AndromedaResult<()>
where
    T: GeneratedResultStreamDescriptorView,
{
    if descriptor.stream_name().trim().is_empty() {
        return contract_error("resolved result stream descriptor name must be non-empty");
    }

    let cardinality = validate_result_cardinality(descriptor.cardinality())?;
    let row_count_requirement = validate_row_count_requirement(descriptor.row_count_requirement())?;
    validate_generated_columns(descriptor.columns())?;

    if row_count_requirement == RowCountRequirementCode::ExactRequired
        && descriptor.row_count_exact().is_none()
    {
        return contract_error("resolved result stream requires row_count_exact");
    }

    if let Some(row_count_exact) = descriptor.row_count_exact()
        && !cardinality_permits_exact(cardinality, row_count_exact)
    {
        return contract_error("resolved result stream row_count_exact violates cardinality");
    }

    if let Some(row_count_max) = descriptor.row_count_max() {
        if !cardinality_permits_max(cardinality, row_count_max) {
            return contract_error("resolved result stream row_count_max violates cardinality");
        }
        if let Some(row_count_exact) = descriptor.row_count_exact()
            && row_count_exact > row_count_max
        {
            return contract_error("resolved result stream row_count_exact exceeds row_count_max");
        }
    }

    Ok(())
}

fn validate_generated_columns<T>(columns: &[T]) -> AndromedaResult<()>
where
    T: GeneratedColumnDescriptorView,
{
    if columns.is_empty() {
        return contract_error("resolved result stream requires at least one typed column");
    }
    if columns.len() > MAX_GENERATED_RESULT_STREAM_COLUMNS {
        return contract_error(format!(
            "resolved result stream columns exceed bounded limit of {MAX_GENERATED_RESULT_STREAM_COLUMNS}"
        ));
    }

    let mut seen_names = BTreeSet::new();
    let mut seen_ordinals = BTreeSet::new();
    for column in columns {
        if column.name().trim().is_empty() {
            return contract_error("resolved result stream column name must be non-empty");
        }
        if column.type_name().trim().is_empty() {
            return contract_error("resolved result stream column type_name must be non-empty");
        }
        if !seen_names.insert(column.name().to_string()) {
            return contract_error("resolved result stream column names must be unique");
        }
        if !seen_ordinals.insert(column.ordinal()) {
            return contract_error("resolved result stream column ordinals must be unique");
        }
    }

    for expected in 0..columns.len() as u32 {
        if !seen_ordinals.contains(&expected) {
            return contract_error(
                "resolved result stream column ordinals must be dense and zero-based",
            );
        }
    }

    Ok(())
}

fn validate_generated_required_permissions<T>(permissions: &[T]) -> AndromedaResult<()>
where
    T: GeneratedRequiredPermissionView,
{
    if permissions.is_empty() {
        return contract_error(
            "resolved procedure manifest requires at least one required permission",
        );
    }

    let mut seen_permissions = BTreeSet::new();
    for permission in permissions {
        if permission.id().trim().is_empty() {
            return contract_error("resolved procedure manifest permission id must be non-empty");
        }
        if permission.id() != permission.id().to_ascii_lowercase() {
            return contract_error("resolved procedure manifest permission id must be lower-case");
        }
        if permission.family().trim().is_empty() {
            return contract_error(
                "resolved procedure manifest permission family must be non-empty",
            );
        }
        if !seen_permissions.insert(permission.id().to_string()) {
            return contract_error(
                "resolved procedure manifest required permissions must be unique",
            );
        }
    }

    Ok(())
}

fn validate_optional_catalog_version(label: &str, version: Option<u64>) -> AndromedaResult<()> {
    if version == Some(0) {
        return contract_error(format!("{label} must be nonzero when present"));
    }

    Ok(())
}

fn validate_required_stats_version(label: &str, version: Option<u64>) -> AndromedaResult<()> {
    match version {
        Some(value) if value != 0 => Ok(()),
        Some(_) => contract_error(format!("{label} must be nonzero")),
        None => contract_error(format!("{label} must be present")),
    }
}

fn validate_completion_status(status: i32) -> AndromedaResult<RpcCompletionStatus> {
    let Ok(code) = u32::try_from(status) else {
        return protocol_error("unknown RPC completion status");
    };

    match RpcCompletionStatus::from_terminal_code(code) {
        Some(status) => Ok(status),
        None if status == 0 => protocol_error("RPC completion status must be specified"),
        None => protocol_error("unknown RPC completion status"),
    }
}

fn validate_transaction_outcome(outcome: i32) -> AndromedaResult<TransactionOutcome> {
    match outcome {
        1 => Ok(TransactionOutcome::NotStarted),
        2 => Ok(TransactionOutcome::Committed),
        3 => Ok(TransactionOutcome::RolledBack),
        4 => Ok(TransactionOutcome::Failed),
        5 => Ok(TransactionOutcome::Cancelled),
        0 => protocol_error("RPC completion transaction outcome must be specified"),
        _ => protocol_error("unknown RPC completion transaction outcome"),
    }
}

fn validate_result_completion_policy<T>(policy: &T) -> AndromedaResult<()>
where
    T: GeneratedResultCompletionPolicyView,
{
    match policy.completion_shape() {
        1..=3 => Ok(()),
        0 => contract_error("generated RPC metadata completion_shape must be specified"),
        _ => protocol_error("unknown generated RPC metadata completion_shape"),
    }
}

fn validate_error_family(error_family: i32) -> AndromedaResult<()> {
    match error_family {
        1..=9 => Ok(()),
        0 => protocol_error("generated error family must be specified"),
        _ => protocol_error("unknown generated error family"),
    }
}

fn validate_transaction_effect(effect: i32) -> AndromedaResult<()> {
    match effect {
        1..=3 => Ok(()),
        0 => protocol_error("generated error transaction_effect must be specified"),
        _ => protocol_error("unknown generated error transaction_effect"),
    }
}

fn validate_retry_disposition(disposition: i32) -> AndromedaResult<GeneratedRetryDispositionCode> {
    match disposition {
        1 => Ok(GeneratedRetryDispositionCode::NotRetryable),
        2 => Ok(GeneratedRetryDispositionCode::Retryable),
        3 => Ok(GeneratedRetryDispositionCode::RetryAfter),
        4 => Ok(GeneratedRetryDispositionCode::Backpressure),
        0 => protocol_error("generated error retry_disposition must be specified"),
        _ => protocol_error("unknown generated error retry_disposition"),
    }
}

fn validate_backpressure_metadata<T>(backpressure: &T) -> AndromedaResult<()>
where
    T: GeneratedBackpressureMetadataView,
{
    if matches!(backpressure.capacity_percent(), Some(percent) if percent > 100) {
        return resource_error("backpressure capacity percent must be <= 100");
    }

    Ok(())
}

fn validate_rpc_execute_arguments<T>(arguments: &[T]) -> AndromedaResult<()>
where
    T: GeneratedRpcExecuteArgumentView,
{
    validate_rpc_execute_arguments_len(arguments.len())?;

    let mut seen_arguments = BTreeSet::new();
    for argument in arguments {
        if argument.name().trim().is_empty() {
            return contract_error("generated RPC execute argument name must be non-empty");
        }
        if argument.type_name().trim().is_empty() {
            return contract_error("generated RPC execute argument type_name must be non-empty");
        }
        validate_rpc_execute_argument_value(argument.value())?;
        if !seen_arguments.insert(argument.name().to_string()) {
            return contract_error("generated RPC execute argument names must be unique");
        }
    }

    Ok(())
}

fn validate_rpc_execute_budget<T>(budget: Option<&T>) -> AndromedaResult<()>
where
    T: GeneratedRpcExecuteRequestBudgetView,
{
    let Some(budget) = budget else {
        return Ok(());
    };

    validate_optional_nonzero(
        "generated RPC execute budget cpu_micros",
        budget.cpu_micros(),
    )?;
    validate_optional_nonzero(
        "generated RPC execute budget memory_bytes",
        budget.memory_bytes(),
    )?;
    validate_optional_nonzero("generated RPC execute budget io_bytes", budget.io_bytes())?;

    if matches!(budget.priority_class(), Some(0)) {
        return contract_error("generated RPC execute budget priority_class must be nonzero");
    }

    Ok(())
}

fn validate_required_invocation_correlation<T>(label: &str, correlation: &T) -> AndromedaResult<()>
where
    T: GeneratedInvocationCorrelationView,
{
    validate_required_nonzero(label, "request_id", correlation.request_id())?;
    validate_required_nonzero(label, "session_id", correlation.session_id())?;

    if matches!(correlation.trace_id(), Some(trace_id) if trace_id.trim().is_empty()) {
        return contract_error(format!("{label} trace_id must be non-empty when present"));
    }

    match correlation.contract_hash() {
        Some(bytes) => {
            let field_label = format!("{label} contract_hash");
            validate_required_contract_hash(&field_label, bytes)?;
        }
        None => {
            return contract_error(format!("{label} requires contract_hash"));
        }
    }

    match correlation.catalog_version() {
        Some(value) if value != 0 => {}
        Some(_) => {
            return contract_error(format!("{label} catalog_version must be nonzero"));
        }
        None => {
            return contract_error(format!("{label} requires catalog_version"));
        }
    }

    let invocation_id_label = format!("{label} invocation_id");
    validate_optional_nonzero(&invocation_id_label, correlation.invocation_id())?;

    match correlation.stats_version() {
        Some(value) if value != 0 => {}
        Some(_) => {
            return contract_error(format!("{label} stats_version must be nonzero"));
        }
        None => {
            return contract_error(format!("{label} requires stats_version"));
        }
    }

    validate_invocation_policy_version_required(label, correlation.expected_policy_version())?;

    Ok(())
}

fn validate_invocation_policy_version_required(
    label: &str,
    expected_policy_version: Option<u64>,
) -> AndromedaResult<()> {
    match expected_policy_version {
        Some(value) if value != 0 => Ok(()),
        Some(_) => contract_error(format!("{label} expected_policy_version must be nonzero")),
        None => contract_error(format!(
            "{label} security context requires expected_policy_version"
        )),
    }
}

fn validate_response_correlation_ids<T>(
    label: &str,
    correlation: &T,
    payload_request_id: Option<u64>,
    payload_session_id: Option<u64>,
) -> AndromedaResult<()>
where
    T: GeneratedInvocationCorrelationView,
{
    if payload_request_id != correlation.request_id() {
        return contract_error(format!(
            "{label} request_id must match invocation correlation"
        ));
    }

    if payload_session_id != correlation.session_id() {
        return contract_error(format!(
            "{label} session_id must match invocation correlation"
        ));
    }

    Ok(())
}

fn validate_required_nonzero(label: &str, field: &str, value: Option<u64>) -> AndromedaResult<()> {
    match value {
        Some(value) if value != 0 => Ok(()),
        Some(_) => contract_error(format!("{label} {field} must be nonzero")),
        None => contract_error(format!("{label} requires {field}")),
    }
}

fn validate_optional_nonzero(label: &str, value: Option<u64>) -> AndromedaResult<()> {
    if matches!(value, Some(0)) {
        return contract_error(format!("{label} must be nonzero when present"));
    }

    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InvocationCorrelationKey {
    request_id: Option<u64>,
    session_id: Option<u64>,
    trace_id: Option<String>,
    contract_hash: Option<Vec<u8>>,
    catalog_version: Option<u64>,
    invocation_id: Option<u64>,
    stats_version: Option<u64>,
    expected_policy_version: Option<u64>,
}

impl InvocationCorrelationKey {
    fn from_correlation<T>(correlation: &T) -> Self
    where
        T: GeneratedInvocationCorrelationView,
    {
        Self {
            request_id: correlation.request_id(),
            session_id: correlation.session_id(),
            trace_id: correlation.trace_id().map(str::to_string),
            contract_hash: correlation.contract_hash().map(<[u8]>::to_vec),
            catalog_version: correlation.catalog_version(),
            invocation_id: correlation.invocation_id(),
            stats_version: correlation.stats_version(),
            expected_policy_version: correlation.expected_policy_version(),
        }
    }
}

#[derive(Default)]
struct InvocationResponseSequenceState {
    correlation: Option<InvocationCorrelationKey>,
    metadata_seen: bool,
    batch_seen: bool,
    terminal_seen: bool,
    completion_shape: Option<i32>,
    structured_payload: StructuredPayloadByteTracker,
    declared_streams: BTreeMap<String, StreamSequenceState>,
}

#[derive(Clone, Debug)]
struct StreamSequenceState {
    next_batch_index: u64,
    rows_emitted: u64,
    row_count_exact: Option<u64>,
    row_count_max: Option<u64>,
    terminal_batch_seen: bool,
}

impl InvocationResponseSequenceState {
    fn observe<T>(&mut self, ordinal: usize, response: &T) -> AndromedaResult<()>
    where
        T: GeneratedInvocationResponseView,
    {
        let Some(correlation) = response.correlation() else {
            return contract_error("generated invocation response sequence requires correlation");
        };
        self.observe_correlation(correlation)?;
        self.validate_response_index(ordinal, response.response_index())?;

        if self.terminal_seen {
            return contract_error(
                "generated invocation response sequence has payload after terminal response",
            );
        }

        match response.payload() {
            Some(GeneratedInvocationResponsePayload::Metadata(metadata)) => {
                self.observe_metadata(metadata)
            }
            Some(GeneratedInvocationResponsePayload::Batch(batch)) => self.observe_batch(batch),
            Some(GeneratedInvocationResponsePayload::Completion(completion)) => {
                self.observe_completion(completion)
            }
            Some(GeneratedInvocationResponsePayload::Error(_)) => self.observe_error(),
            None => contract_error(
                "generated invocation response sequence requires typed response payload",
            ),
        }
    }

    fn observe_correlation<T>(&mut self, correlation: &T) -> AndromedaResult<()>
    where
        T: GeneratedInvocationCorrelationView,
    {
        let current = InvocationCorrelationKey::from_correlation(correlation);
        if let Some(expected) = self.correlation.as_ref() {
            if expected != &current {
                return contract_error(
                    "generated invocation response sequence correlation must remain stable",
                );
            }
        } else {
            self.correlation = Some(current);
        }

        Ok(())
    }

    fn validate_response_index(
        &self,
        ordinal: usize,
        response_index: Option<u64>,
    ) -> AndromedaResult<()> {
        let Some(response_index) = response_index else {
            return contract_error(
                "generated invocation response sequence requires response_index on every frame",
            );
        };

        if response_index != ordinal as u64 {
            return contract_error(
                "generated invocation response sequence response_index must be gap-free",
            );
        }

        Ok(())
    }

    fn observe_metadata<T>(&mut self, metadata: &T) -> AndromedaResult<()>
    where
        T: GeneratedRpcMetadataView,
    {
        if self.metadata_seen {
            return contract_error(
                "generated invocation response sequence must not repeat metadata",
            );
        }
        if self.terminal_seen {
            return contract_error(
                "generated invocation response sequence metadata must precede terminal response",
            );
        }

        self.metadata_seen = true;
        let Some(policy) = metadata.completion_policy() else {
            return contract_error(
                "generated invocation response sequence metadata requires completion_policy",
            );
        };
        self.completion_shape = Some(policy.completion_shape());

        if policy.completion_shape() == 3 && !metadata.result_streams().is_empty() {
            return contract_error(
                "generated invocation response sequence mutation-only metadata must not declare result streams",
            );
        }

        for descriptor in metadata.result_streams() {
            self.declared_streams.insert(
                descriptor.stream_name().to_string(),
                StreamSequenceState::new(descriptor.row_count_exact(), descriptor.row_count_max()),
            );
        }

        Ok(())
    }

    fn observe_batch<T>(&mut self, batch: &T) -> AndromedaResult<()>
    where
        T: GeneratedRpcBatchView,
    {
        if !self.metadata_seen {
            return contract_error(
                "generated invocation response sequence batch must follow metadata",
            );
        }

        let Some(stream) = self.declared_streams.get_mut(batch.result_name()) else {
            return contract_error(
                "generated invocation response sequence batch result_name must be declared by metadata",
            );
        };

        stream.observe_batch(batch)?;
        self.structured_payload
            .observe_batch_payload(batch.structured_payload())?;
        stream.validate_observed_batch_counts(batch)?;
        stream.observe_terminal_batch(batch.terminal_batch());
        self.batch_seen = true;

        Ok(())
    }

    fn observe_completion<T>(&mut self, completion: &T) -> AndromedaResult<()>
    where
        T: GeneratedRpcCompletionView,
    {
        if self.terminal_seen {
            return contract_error(
                "generated invocation response sequence must contain exactly one terminal response",
            );
        }
        if !self.metadata_seen {
            return contract_error(
                "generated invocation response sequence completion must follow metadata",
            );
        }

        self.validate_completion_policy()?;
        self.validate_completion_row_counts(completion)?;
        self.terminal_seen = true;
        Ok(())
    }

    fn observe_error(&mut self) -> AndromedaResult<()> {
        if self.terminal_seen {
            return contract_error(
                "generated invocation response sequence must contain exactly one terminal response",
            );
        }

        self.terminal_seen = true;
        Ok(())
    }

    fn validate_completion_row_counts<T>(&self, completion: &T) -> AndromedaResult<()>
    where
        T: GeneratedRpcCompletionView,
    {
        let mut seen_summaries = BTreeSet::new();
        for summary in completion.result_row_counts() {
            if !seen_summaries.insert(summary.result_name().to_string()) {
                return contract_error(
                    "generated invocation response completion result summaries must be unique",
                );
            }

            let Some(stream) = self.declared_streams.get(summary.result_name()) else {
                return contract_error(
                    "generated invocation response completion result_name must be declared by metadata",
                );
            };

            stream.validate_completion_summary(summary)?;
        }

        for (result_name, stream) in &self.declared_streams {
            if !seen_summaries.contains(result_name) {
                return contract_error(
                    "generated invocation response completion must summarize every declared result stream",
                );
            }

            stream.validate_completion_ready()?;
        }

        Ok(())
    }

    fn validate_completion_policy(&self) -> AndromedaResult<()> {
        match self.completion_shape {
            Some(1) => {
                if self.batch_seen {
                    Ok(())
                } else {
                    contract_error(
                        "generated invocation response sequence requires row batch before completion",
                    )
                }
            }
            Some(2) => Ok(()),
            Some(3) => {
                if self.batch_seen || !self.declared_streams.is_empty() {
                    contract_error(
                        "generated invocation response sequence mutation-only completion must not include row streams",
                    )
                } else {
                    Ok(())
                }
            }
            Some(0) => contract_error(
                "generated invocation response sequence completion_shape must be specified",
            ),
            _ => protocol_error("unknown generated invocation response sequence completion_shape"),
        }
    }

    fn finish(self) -> AndromedaResult<()> {
        if !self.terminal_seen {
            return contract_error(
                "generated invocation response sequence requires a terminal response",
            );
        }

        Ok(())
    }
}

impl StreamSequenceState {
    fn new(row_count_exact: Option<u64>, row_count_max: Option<u64>) -> Self {
        Self {
            next_batch_index: 0,
            rows_emitted: 0,
            row_count_exact,
            row_count_max,
            terminal_batch_seen: false,
        }
    }

    fn observe_batch<T>(&mut self, batch: &T) -> AndromedaResult<()>
    where
        T: GeneratedRpcBatchView,
    {
        if self.terminal_batch_seen {
            return contract_error(
                "generated invocation response sequence has batch after terminal batch",
            );
        }

        if batch.batch_index() != self.next_batch_index {
            return contract_error(
                "generated invocation response sequence batch_index must be gap-free per result stream",
            );
        }
        self.next_batch_index = self.next_batch_index.checked_add(1).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "generated invocation response sequence batch_index overflow",
            )
        })?;

        self.rows_emitted = self
            .rows_emitted
            .checked_add(batch.rows_emitted())
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    "generated invocation response sequence rows_emitted overflow",
                )
            })?;

        Ok(())
    }

    fn validate_observed_batch_counts<T>(&self, batch: &T) -> AndromedaResult<()>
    where
        T: GeneratedRpcBatchView,
    {
        self.validate_batch_row_count_exact(batch.row_count_exact())?;
        self.validate_row_count_max()
    }

    fn validate_batch_row_count_exact(&self, row_count_exact: Option<u64>) -> AndromedaResult<()> {
        if let Some(expected_exact) = self.row_count_exact {
            if row_count_exact != Some(expected_exact) {
                return contract_error(
                    "generated invocation response sequence batch row_count_exact must match metadata",
                );
            }
            if self.rows_emitted > expected_exact {
                return contract_error(
                    "generated invocation response sequence rows_emitted exceeds row_count_exact",
                );
            }
        } else if row_count_exact.is_some() {
            return contract_error(
                "generated invocation response sequence batch must not introduce row_count_exact absent from metadata",
            );
        }

        Ok(())
    }

    fn validate_row_count_max(&self) -> AndromedaResult<()> {
        if let Some(max) = self.row_count_max
            && self.rows_emitted > max
        {
            return contract_error(
                "generated invocation response sequence rows_emitted exceeds row_count_max",
            );
        }

        Ok(())
    }

    fn observe_terminal_batch(&mut self, terminal_batch: bool) {
        if terminal_batch {
            self.terminal_batch_seen = true;
        }
    }

    fn validate_completion_summary<T>(&self, summary: &T) -> AndromedaResult<()>
    where
        T: GeneratedResultRowCountSummaryView,
    {
        if summary.rows_emitted() != self.rows_emitted {
            return contract_error(
                "generated invocation response completion rows_emitted must match batches",
            );
        }

        if let Some(expected_exact) = self.row_count_exact
            && summary.row_count_exact() != Some(expected_exact)
        {
            return contract_error(
                "generated invocation response completion row_count_exact must match metadata",
            );
        }
        if self.row_count_exact.is_none() && summary.row_count_exact().is_some() {
            return contract_error(
                "generated invocation response completion must not introduce row_count_exact absent from metadata",
            );
        }

        Ok(())
    }

    fn validate_completion_ready(&self) -> AndromedaResult<()> {
        if self.rows_emitted > 0 && !self.terminal_batch_seen {
            return contract_error(
                "generated invocation response completion requires terminal batch before completion",
            );
        }

        if let Some(expected_exact) = self.row_count_exact
            && self.rows_emitted != expected_exact
        {
            return contract_error(
                "generated invocation response sequence rows_emitted must equal row_count_exact before completion",
            );
        }

        Ok(())
    }
}

fn project_payload_kind(payload_kind: i32) -> AndromedaResult<PayloadKind> {
    let Ok(code) = u32::try_from(payload_kind) else {
        return protocol_error("unknown generated frame envelope payload_kind");
    };

    PayloadKind::try_from(code)
}

fn project_contract_hash_for_payload_kind(
    label: &str,
    payload_kind: PayloadKind,
    bytes: &[u8],
) -> AndromedaResult<ContractHash> {
    if bytes.is_empty() && !payload_kind.requires_contract_hash() {
        return Ok(ContractHash::zero());
    }

    if payload_kind.requires_contract_hash() {
        validate_required_contract_hash(label, bytes)?;
    } else {
        validate_optional_contract_hash(label, Some(bytes))?;
    }

    ContractHash::from_slice(bytes)
}

fn project_generated_structured_object_fields<T>(
    fields: &[T],
) -> AndromedaResult<Vec<ColumnDescriptor>>
where
    T: GeneratedColumnDescriptorView,
{
    if fields.is_empty() {
        return contract_error("generated StructuredObjectHeader requires at least one field");
    }
    if fields.len() > MAX_GENERATED_STRUCTURED_OBJECT_FIELDS {
        return contract_error(format!(
            "generated StructuredObjectHeader fields exceed bounded limit of {MAX_GENERATED_STRUCTURED_OBJECT_FIELDS}"
        ));
    }

    let mut seen_names = BTreeSet::new();
    let mut seen_ordinals = BTreeSet::new();
    let mut projected = Vec::with_capacity(fields.len());
    for field in fields {
        if field.name().trim().is_empty() {
            return contract_error("generated StructuredObjectHeader field name must be non-empty");
        }
        if !seen_names.insert(field.name().to_string()) {
            return contract_error("generated StructuredObjectHeader field names must be unique");
        }
        if !seen_ordinals.insert(field.ordinal()) {
            return contract_error(
                "generated StructuredObjectHeader field ordinals must be unique",
            );
        }
        projected.push(ColumnDescriptor {
            name: field.name().to_string(),
            data_type: project_generated_structured_object_type(field.type_name())?,
            ordinal: field.ordinal(),
        });
    }

    for expected in 0..fields.len() as u32 {
        if !seen_ordinals.contains(&expected) {
            return contract_error(
                "generated StructuredObjectHeader field ordinals must be dense and zero-based",
            );
        }
    }

    Ok(projected)
}

fn project_generated_structured_object_type(type_name: &str) -> AndromedaResult<TypeDescriptor> {
    if type_name.trim().is_empty() {
        return contract_error(
            "generated StructuredObjectHeader field type_name must be non-empty",
        );
    }

    let scalar = match type_name {
        "i8" => ScalarType::I8,
        "i16" => ScalarType::I16,
        "i32" => ScalarType::I32,
        "i64" => ScalarType::I64,
        "i128" => ScalarType::I128,
        "u8" => ScalarType::U8,
        "u16" => ScalarType::U16,
        "u32" => ScalarType::U32,
        "u64" => ScalarType::U64,
        "u128" => ScalarType::U128,
        "bool" => ScalarType::Bool,
        _ => {
            return contract_error(format!(
                "generated StructuredObjectHeader field type_name '{type_name}' is not in the minimal contract-safe projection set"
            ));
        }
    };

    Ok(TypeDescriptor::required(scalar))
}

fn project_generated_structured_object_layout(
    layout: i32,
) -> AndromedaResult<StructuredObjectLayout> {
    match layout {
        1 => Ok(StructuredObjectLayout::RowMajor),
        2 => Ok(StructuredObjectLayout::ColumnMajor),
        3 => Ok(StructuredObjectLayout::Hybrid),
        0 => contract_error("generated StructuredObjectHeader layout must be specified"),
        _ => protocol_error("unknown generated StructuredObjectHeader layout"),
    }
}

fn project_generated_structured_object_row_count_policy(
    policy: i32,
) -> AndromedaResult<RowCountPolicy> {
    match policy {
        1 => Ok(RowCountPolicy::UnknownAllowed),
        2 => Ok(RowCountPolicy::ExactIfKnown),
        3 => Ok(RowCountPolicy::ExactRequired),
        0 => contract_error("generated StructuredObjectHeader row_count_policy must be specified"),
        _ => protocol_error("unknown generated StructuredObjectHeader row_count_policy"),
    }
}

fn project_generated_structured_object_row_count_exact(
    row_count_exact: u64,
) -> AndromedaResult<Option<u64>> {
    if row_count_exact == 0 {
        return contract_error(
            "generated StructuredObjectHeader row_count_exact zero is ambiguous until the protobuf field has explicit presence; projection fails closed",
        );
    }

    Ok(Some(row_count_exact))
}

fn validate_generated_structured_object_payload_limits<T>(header: &T) -> AndromedaResult<()>
where
    T: GeneratedStructuredObjectHeaderView,
{
    if header.payload_length() > MAX_GENERATED_STRUCTURED_OBJECT_PAYLOAD_BYTES {
        return protocol_error(format!(
            "generated StructuredObjectHeader payload_length exceeds bounded limit of {MAX_GENERATED_STRUCTURED_OBJECT_PAYLOAD_BYTES}"
        ));
    }

    if let Some(max_payload_length) = header.max_payload_length()
        && max_payload_length > MAX_GENERATED_STRUCTURED_OBJECT_PAYLOAD_BYTES
    {
        return contract_error(format!(
            "generated StructuredObjectHeader max_payload_length exceeds bounded limit of {MAX_GENERATED_STRUCTURED_OBJECT_PAYLOAD_BYTES}"
        ));
    }

    Ok(())
}

fn validate_unresolved_response_fields<T>(response: &T) -> AndromedaResult<()>
where
    T: GeneratedCatalogManifestResolutionResponseView,
{
    if response.manifest().is_some() {
        return contract_error("non-resolved catalog manifest response must not carry a manifest");
    }

    if response.resolved_contract_hash().is_some() {
        return contract_error(
            "non-resolved catalog manifest response must not carry resolved_contract_hash",
        );
    }

    if response.resolved_catalog_version().is_some() {
        return contract_error(
            "non-resolved catalog manifest response must not carry resolved_catalog_version",
        );
    }

    Ok(())
}

fn validate_resolution_status(status: i32) -> AndromedaResult<ResolutionStatusCode> {
    match status {
        1 => Ok(ResolutionStatusCode::Resolved),
        2 => Ok(ResolutionStatusCode::NotFound),
        3 => Ok(ResolutionStatusCode::CatalogVersionMismatch),
        4 => Ok(ResolutionStatusCode::ContractHashMismatch),
        5 => Ok(ResolutionStatusCode::NotSourceGeneratorReady),
        6 => Ok(ResolutionStatusCode::PermissionDenied),
        7 => Ok(ResolutionStatusCode::Unsupported),
        8 => Ok(ResolutionStatusCode::Malformed),
        9 => Ok(ResolutionStatusCode::Internal),
        10 => Ok(ResolutionStatusCode::CatalogNotReady),
        11 => Ok(ResolutionStatusCode::AuthRequired),
        0 => protocol_error("catalog manifest resolution status must be specified"),
        _ => protocol_error("unknown catalog manifest resolution status"),
    }
}

fn validate_result_cardinality(cardinality: i32) -> AndromedaResult<ResultCardinalityCode> {
    match cardinality {
        1 => Ok(ResultCardinalityCode::ZeroOrMore),
        2 => Ok(ResultCardinalityCode::ZeroOrOne),
        3 => Ok(ResultCardinalityCode::OneOrMore),
        4 => Ok(ResultCardinalityCode::ExactlyOne),
        0 => contract_error("resolved result stream cardinality must be specified"),
        _ => protocol_error("unknown resolved result stream cardinality"),
    }
}

fn validate_row_count_requirement(requirement: i32) -> AndromedaResult<RowCountRequirementCode> {
    match requirement {
        1 => Ok(RowCountRequirementCode::UnknownAllowed),
        2 => Ok(RowCountRequirementCode::ExactIfKnown),
        3 => Ok(RowCountRequirementCode::ExactRequired),
        0 => contract_error("resolved result stream row_count_requirement must be specified"),
        _ => protocol_error("unknown resolved result stream row_count_requirement"),
    }
}

fn cardinality_permits_exact(cardinality: ResultCardinalityCode, exact: u64) -> bool {
    match cardinality {
        ResultCardinalityCode::ZeroOrMore => true,
        ResultCardinalityCode::ZeroOrOne => exact <= 1,
        ResultCardinalityCode::OneOrMore => exact >= 1,
        ResultCardinalityCode::ExactlyOne => exact == 1,
    }
}

fn cardinality_permits_max(cardinality: ResultCardinalityCode, max: u64) -> bool {
    match cardinality {
        ResultCardinalityCode::ZeroOrMore => true,
        ResultCardinalityCode::ZeroOrOne => max <= 1,
        ResultCardinalityCode::OneOrMore => max >= 1,
        ResultCardinalityCode::ExactlyOne => max == 1,
    }
}

fn contract_error<T>(message: impl Into<String>) -> AndromedaResult<T> {
    Err(AndromedaError::new(
        AndromedaErrorKind::Contract,
        message.into(),
    ))
}

fn protocol_error<T>(message: impl Into<String>) -> AndromedaResult<T> {
    Err(AndromedaError::new(
        AndromedaErrorKind::Protocol,
        message.into(),
    ))
}

fn resource_error<T>(message: impl Into<String>) -> AndromedaResult<T> {
    Err(AndromedaError::new(
        AndromedaErrorKind::Resource,
        message.into(),
    ))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GeneratedRetryDispositionCode {
    NotRetryable,
    Retryable,
    RetryAfter,
    Backpressure,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResolutionStatusCode {
    Resolved,
    NotFound,
    CatalogVersionMismatch,
    ContractHashMismatch,
    NotSourceGeneratorReady,
    PermissionDenied,
    Unsupported,
    Malformed,
    Internal,
    CatalogNotReady,
    AuthRequired,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResultCardinalityCode {
    ZeroOrMore,
    ZeroOrOne,
    OneOrMore,
    ExactlyOne,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RowCountRequirementCode {
    UnknownAllowed,
    ExactIfKnown,
    ExactRequired,
}

impl GeneratedProtocolVersionView for GeneratedProtocolVersion {
    fn major(&self) -> u32 {
        self.major
    }

    fn minor(&self) -> u32 {
        self.minor
    }
}

impl GeneratedFrameEnvelopeView for GeneratedFrameEnvelope {
    type ProtocolVersion = GeneratedProtocolVersion;

    fn protocol_version(&self) -> Option<&Self::ProtocolVersion> {
        self.protocol_version.as_ref()
    }

    fn contract_hash(&self) -> &[u8] {
        &self.contract_hash
    }

    fn catalog_version(&self) -> u64 {
        self.catalog_version
    }

    fn request_id(&self) -> u64 {
        self.request_id
    }

    fn session_id(&self) -> u64 {
        self.session_id
    }

    fn tx_id(&self) -> Option<u64> {
        self.tx_id
    }

    fn payload_kind(&self) -> i32 {
        self.payload_kind
    }

    fn payload(&self) -> &[u8] {
        &self.payload
    }
}

impl GeneratedColumnDescriptorView for GeneratedColumnDescriptor {
    fn name(&self) -> &str {
        &self.name
    }

    fn ordinal(&self) -> u32 {
        self.ordinal
    }

    fn type_name(&self) -> &str {
        &self.type_name
    }
}

impl GeneratedResultStreamDescriptorView for GeneratedResultStreamDescriptor {
    type Column = GeneratedColumnDescriptor;

    fn stream_name(&self) -> &str {
        &self.stream_name
    }

    fn columns(&self) -> &[Self::Column] {
        &self.columns
    }

    fn cardinality(&self) -> i32 {
        self.cardinality
    }

    fn row_count_requirement(&self) -> i32 {
        self.row_count_requirement
    }

    fn row_count_exact(&self) -> Option<u64> {
        self.row_count_exact
    }

    fn row_count_max(&self) -> Option<u64> {
        self.row_count_max
    }
}

impl GeneratedResultCompletionPolicyView for GeneratedResultCompletionPolicy {
    fn completion_shape(&self) -> i32 {
        self.completion_shape
    }
}

impl GeneratedRpcMetadataView for GeneratedRpcMetadata {
    type ResultStream = GeneratedResultStreamDescriptor;
    type CompletionPolicy = GeneratedResultCompletionPolicy;

    fn result_streams(&self) -> &[Self::ResultStream] {
        &self.result_streams
    }

    fn completion_policy(&self) -> Option<&Self::CompletionPolicy> {
        self.completion_policy.as_ref()
    }
}

impl GeneratedRpcBatchView for GeneratedRpcBatch {
    fn result_name(&self) -> &str {
        &self.result_name
    }

    fn batch_index(&self) -> u64 {
        self.batch_index
    }

    fn rows_emitted(&self) -> u64 {
        self.rows_emitted
    }

    fn structured_payload(&self) -> &[u8] {
        &self.structured_payload
    }

    fn row_count_exact(&self) -> Option<u64> {
        self.row_count_exact
    }

    fn terminal_batch(&self) -> bool {
        self.terminal_batch
    }
}

impl GeneratedResultRowCountSummaryView for GeneratedResultRowCountSummary {
    fn result_name(&self) -> &str {
        &self.result_name
    }

    fn rows_emitted(&self) -> u64 {
        self.rows_emitted
    }

    fn row_count_exact(&self) -> Option<u64> {
        self.row_count_exact
    }
}

impl GeneratedRpcCompletionView for GeneratedRpcCompletion {
    type ResultRowCountSummary = GeneratedResultRowCountSummary;

    fn status(&self) -> i32 {
        self.status
    }

    fn rows_affected(&self) -> Option<u64> {
        self.rows_affected
    }

    fn tx_id(&self) -> Option<u64> {
        self.tx_id
    }

    fn request_id(&self) -> Option<u64> {
        self.request_id
    }

    fn session_id(&self) -> Option<u64> {
        self.session_id
    }

    fn trace_id(&self) -> Option<&str> {
        self.trace_id.as_deref()
    }

    fn transaction_outcome(&self) -> i32 {
        self.transaction_outcome
    }

    fn durable_lsn(&self) -> Option<u64> {
        self.durable_lsn
    }

    fn result_row_counts(&self) -> &[Self::ResultRowCountSummary] {
        &self.result_row_counts
    }
}
