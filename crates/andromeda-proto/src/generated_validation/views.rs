use andromeda_proto_wire::generated_validation as wire;

use crate::generated::{contract, protocol};

impl wire::GeneratedProtocolVersionView for protocol::v1::ProtocolVersion {
    fn major(&self) -> u32 {
        self.major
    }

    fn minor(&self) -> u32 {
        self.minor
    }
}

impl wire::GeneratedFrameEnvelopeView for protocol::v1::FrameEnvelope {
    type ProtocolVersion = protocol::v1::ProtocolVersion;

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

impl wire::GeneratedColumnDescriptorView for contract::v1::ColumnDescriptor {
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

impl wire::GeneratedResultStreamDescriptorView for contract::v1::ResultStreamDescriptor {
    type Column = contract::v1::ColumnDescriptor;

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

impl wire::GeneratedRequiredPermissionView for contract::v1::RequiredPermission {
    fn id(&self) -> &str {
        &self.id
    }

    fn family(&self) -> &str {
        &self.family
    }
}

impl wire::GeneratedProtocolLayoutView for contract::v1::ProtocolLayout {
    fn descriptor_set_hash(&self) -> &[u8] {
        &self.descriptor_set_hash
    }

    fn frame_envelope_hash(&self) -> &[u8] {
        &self.frame_envelope_hash
    }

    fn protocol_package(&self) -> &str {
        &self.protocol_package
    }

    fn contract_package(&self) -> &str {
        &self.contract_package
    }
}

impl wire::GeneratedProcedureManifestView for contract::v1::ProcedureManifest {
    type ProtocolLayout = contract::v1::ProtocolLayout;
    type ResultStream = contract::v1::ResultStreamDescriptor;
    type RequiredPermission = contract::v1::RequiredPermission;

    fn procedure_id(&self) -> u64 {
        self.procedure_id
    }

    fn procedure_name(&self) -> &str {
        &self.procedure_name
    }

    fn contract_hash(&self) -> &[u8] {
        &self.contract_hash
    }

    fn catalog_version(&self) -> u64 {
        self.catalog_version
    }

    fn protocol_layout(&self) -> Option<&Self::ProtocolLayout> {
        self.protocol_layout.as_ref()
    }

    fn result_streams(&self) -> &[Self::ResultStream] {
        &self.result_streams
    }

    fn policy_version(&self) -> &[u8] {
        &self.policy_version
    }

    fn required_permissions(&self) -> &[Self::RequiredPermission] {
        &self.required_permissions
    }

    fn stats_version(&self) -> Option<u64> {
        self.stats_version
    }
}

impl wire::GeneratedCatalogManifestResolutionRequestView
    for contract::v1::CatalogProcedureManifestResolutionRequest
{
    fn protocol_major(&self) -> u32 {
        self.protocol_major
    }

    fn protocol_minor(&self) -> u32 {
        self.protocol_minor
    }

    fn request_id(&self) -> u64 {
        self.request_id
    }

    fn selector(&self) -> Option<wire::GeneratedCatalogManifestResolutionSelector<'_>> {
        use contract::v1::catalog_procedure_manifest_resolution_request::Selector;

        match self.selector.as_ref() {
            Some(Selector::ProcedureId(procedure_id)) => {
                Some(wire::GeneratedCatalogManifestResolutionSelector::ProcedureId(*procedure_id))
            },
            Some(Selector::ProcedureName(procedure_name)) => Some(
                wire::GeneratedCatalogManifestResolutionSelector::ProcedureName(procedure_name),
            ),
            None => None,
        }
    }

    fn expected_contract_hash(&self) -> Option<&[u8]> {
        self.expected_contract_hash.as_deref()
    }

    fn expected_catalog_version(&self) -> Option<u64> {
        self.expected_catalog_version
    }
}

impl wire::GeneratedCatalogManifestResolutionResponseView
    for contract::v1::CatalogProcedureManifestResolutionResponse
{
    type Manifest = contract::v1::ProcedureManifest;

    fn protocol_major(&self) -> u32 {
        self.protocol_major
    }

    fn protocol_minor(&self) -> u32 {
        self.protocol_minor
    }

    fn request_id(&self) -> u64 {
        self.request_id
    }

    fn status(&self) -> i32 {
        self.status
    }

    fn manifest(&self) -> Option<&Self::Manifest> {
        self.manifest.as_ref()
    }

    fn resolved_contract_hash(&self) -> Option<&[u8]> {
        self.resolved_contract_hash.as_deref()
    }

    fn resolved_catalog_version(&self) -> Option<u64> {
        self.resolved_catalog_version
    }

    fn current_catalog_version(&self) -> Option<u64> {
        self.current_catalog_version
    }
}

impl wire::GeneratedResultCompletionPolicyView for protocol::v1::ResultCompletionPolicy {
    fn completion_shape(&self) -> i32 {
        self.completion_shape
    }
}

impl wire::GeneratedRpcMetadataView for protocol::v1::RpcMetadata {
    type ResultStream = contract::v1::ResultStreamDescriptor;
    type CompletionPolicy = protocol::v1::ResultCompletionPolicy;

    fn result_streams(&self) -> &[Self::ResultStream] {
        &self.result_streams
    }

    fn completion_policy(&self) -> Option<&Self::CompletionPolicy> {
        self.completion_policy.as_ref()
    }
}

impl wire::GeneratedRpcBatchView for protocol::v1::RpcBatch {
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

impl wire::GeneratedResultRowCountSummaryView
    for protocol::v1::rpc_completion::ResultRowCountSummary
{
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

impl wire::GeneratedRpcCompletionView for protocol::v1::RpcCompletion {
    type ResultRowCountSummary = protocol::v1::rpc_completion::ResultRowCountSummary;

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

impl wire::GeneratedBackpressureMetadataView
    for protocol::v1::error_envelope::BackpressureMetadata
{
    fn retry_after_ms(&self) -> Option<u64> {
        self.retry_after_ms
    }

    fn capacity_percent(&self) -> Option<u32> {
        self.capacity_percent
    }
}

impl wire::GeneratedErrorEnvelopeView for protocol::v1::ErrorEnvelope {
    type Backpressure = protocol::v1::error_envelope::BackpressureMetadata;

    fn request_id(&self) -> Option<u64> {
        self.request_id
    }

    fn session_id(&self) -> Option<u64> {
        self.session_id
    }

    fn trace_id(&self) -> Option<&str> {
        self.trace_id.as_deref()
    }

    fn family(&self) -> i32 {
        self.family
    }

    fn code(&self) -> &str {
        &self.code
    }

    fn message(&self) -> &str {
        &self.message
    }

    fn transaction_effect(&self) -> i32 {
        self.transaction_effect
    }

    fn retry_disposition(&self) -> i32 {
        self.retry_disposition
    }

    fn retry_after_ms(&self) -> Option<u64> {
        self.retry_after_ms
    }

    fn backpressure(&self) -> Option<&Self::Backpressure> {
        self.backpressure.as_ref()
    }
}

impl wire::GeneratedRpcExecuteArgumentView for protocol::v1::rpc_execute_request::Argument {
    fn name(&self) -> &str {
        &self.name
    }

    fn type_name(&self) -> &str {
        &self.type_name
    }

    fn value(&self) -> &[u8] {
        &self.value
    }
}

impl wire::GeneratedRpcExecuteRequestBudgetView
    for protocol::v1::rpc_execute_request::RequestBudget
{
    fn cpu_micros(&self) -> Option<u64> {
        self.cpu_micros
    }

    fn memory_bytes(&self) -> Option<u64> {
        self.memory_bytes
    }

    fn io_bytes(&self) -> Option<u64> {
        self.io_bytes
    }

    fn priority_class(&self) -> Option<u32> {
        self.priority_class
    }
}

impl wire::GeneratedRpcExecuteRequestView for protocol::v1::RpcExecuteRequest {
    type Argument = protocol::v1::rpc_execute_request::Argument;
    type Budget = protocol::v1::rpc_execute_request::RequestBudget;

    fn procedure_name(&self) -> &str {
        &self.procedure_name
    }

    fn expected_contract_hash(&self) -> &[u8] {
        &self.expected_contract_hash
    }

    fn expected_catalog_version(&self) -> u64 {
        self.expected_catalog_version
    }

    fn surface_scope(&self) -> &str {
        &self.surface_scope
    }

    fn arguments(&self) -> &[Self::Argument] {
        &self.arguments
    }

    fn budget(&self) -> Option<&Self::Budget> {
        self.budget.as_ref()
    }

    fn expected_stats_version(&self) -> Option<u64> {
        self.expected_stats_version
    }
}

impl wire::GeneratedInvocationCorrelationView for protocol::v1::InvocationCorrelation {
    fn request_id(&self) -> Option<u64> {
        self.request_id
    }

    fn session_id(&self) -> Option<u64> {
        self.session_id
    }

    fn trace_id(&self) -> Option<&str> {
        self.trace_id.as_deref()
    }

    fn contract_hash(&self) -> Option<&[u8]> {
        self.contract_hash.as_deref()
    }

    fn catalog_version(&self) -> Option<u64> {
        self.catalog_version
    }

    fn invocation_id(&self) -> Option<u64> {
        self.invocation_id
    }

    fn stats_version(&self) -> Option<u64> {
        self.stats_version
    }

    fn expected_policy_version(&self) -> Option<u64> {
        self.expected_policy_version
    }
}

impl wire::GeneratedInvocationRequestView for protocol::v1::InvocationRequest {
    type Correlation = protocol::v1::InvocationCorrelation;
    type ExecuteRequest = protocol::v1::RpcExecuteRequest;

    fn correlation(&self) -> Option<&Self::Correlation> {
        self.correlation.as_ref()
    }

    fn execute_request(&self) -> Option<&Self::ExecuteRequest> {
        self.execute_request.as_ref()
    }
}

impl wire::GeneratedInvocationResponseView for protocol::v1::InvocationResponse {
    type Correlation = protocol::v1::InvocationCorrelation;
    type Metadata = protocol::v1::RpcMetadata;
    type Batch = protocol::v1::RpcBatch;
    type Completion = protocol::v1::RpcCompletion;
    type Error = protocol::v1::ErrorEnvelope;

    fn correlation(&self) -> Option<&Self::Correlation> {
        self.correlation.as_ref()
    }

    fn response_index(&self) -> Option<u64> {
        self.response_index
    }

    fn payload(
        &self,
    ) -> Option<
        wire::GeneratedInvocationResponsePayload<
            '_,
            Self::Metadata,
            Self::Batch,
            Self::Completion,
            Self::Error,
        >,
    > {
        use protocol::v1::invocation_response::Response;

        match self.response.as_ref() {
            Some(Response::Metadata(metadata)) => {
                Some(wire::GeneratedInvocationResponsePayload::Metadata(metadata))
            },
            Some(Response::Batch(batch)) => {
                Some(wire::GeneratedInvocationResponsePayload::Batch(batch))
            },
            Some(Response::Completion(completion)) => Some(
                wire::GeneratedInvocationResponsePayload::Completion(completion),
            ),
            Some(Response::Error(error)) => {
                Some(wire::GeneratedInvocationResponsePayload::Error(error))
            },
            None => None,
        }
    }
}

impl wire::GeneratedStructuredObjectHeaderView for contract::v1::StructuredObjectHeader {
    type Column = contract::v1::ColumnDescriptor;

    fn name(&self) -> &str {
        &self.name
    }

    fn contract_hash(&self) -> &[u8] {
        &self.contract_hash
    }

    fn descriptor_hash(&self) -> &[u8] {
        &self.descriptor_hash
    }

    fn row_count_exact(&self) -> u64 {
        self.row_count_exact
    }

    fn column_count(&self) -> u32 {
        self.column_count
    }

    fn layout(&self) -> i32 {
        self.layout
    }

    fn payload_length(&self) -> u64 {
        self.payload_length
    }

    fn payload_checksum(&self) -> Option<u64> {
        self.payload_checksum
    }

    fn max_payload_length(&self) -> Option<u64> {
        self.max_payload_length
    }

    fn fields(&self) -> &[Self::Column] {
        &self.fields
    }

    fn row_count_policy(&self) -> i32 {
        self.row_count_policy
    }
}
