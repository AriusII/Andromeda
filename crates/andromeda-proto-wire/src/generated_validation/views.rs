use crate::{
    GeneratedColumnDescriptor, GeneratedFrameEnvelope, GeneratedProtocolVersion,
    GeneratedResultCompletionPolicy, GeneratedResultRowCountSummary,
    GeneratedResultStreamDescriptor, GeneratedRpcBatch, GeneratedRpcCompletion,
    GeneratedRpcMetadata,
};

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
