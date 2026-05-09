#![allow(dead_code)]

use andromeda_proto_wire::generated_validation::{
    GeneratedBackpressureMetadataView, GeneratedColumnDescriptorView, GeneratedErrorEnvelopeView,
    GeneratedInvocationCorrelationView, GeneratedInvocationRequestView,
    GeneratedInvocationResponsePayload, GeneratedInvocationResponseView,
    GeneratedResultCompletionPolicyView, GeneratedResultRowCountSummaryView,
    GeneratedResultStreamDescriptorView, GeneratedRpcBatchView, GeneratedRpcCompletionView,
    GeneratedRpcExecuteArgumentView, GeneratedRpcExecuteRequestBudgetView,
    GeneratedRpcExecuteRequestView, GeneratedRpcMetadataView,
};
use andromeda_types::ContractHash;

pub(crate) mod result_stream_descriptor {
    #[derive(Clone, Copy)]
    pub(crate) enum Cardinality {
        ZeroOrMore = 1,
        ExactlyOne = 4,
    }

    #[derive(Clone, Copy)]
    pub(crate) enum RowCountRequirement {
        ExactIfKnown = 2,
        ExactRequired = 3,
    }
}

pub(crate) mod result_completion_policy {
    #[derive(Clone, Copy)]
    pub(crate) enum CompletionShape {
        RequiresRowBatch = 1,
        AllowsZeroRowCompletion = 2,
        MutationOnly = 3,
    }
}

pub(crate) mod rpc_completion {
    #[derive(Clone, Copy)]
    pub(crate) enum Status {
        Committed = 1,
    }

    #[derive(Clone, Copy)]
    pub(crate) enum TransactionOutcome {
        Committed = 2,
    }

    #[derive(Clone)]
    pub(crate) struct ResultRowCountSummary {
        pub(crate) result_name: String,
        pub(crate) rows_emitted: u64,
        pub(crate) row_count_exact: Option<u64>,
    }
}

pub(crate) mod rpc_execute_request {
    #[derive(Clone)]
    pub(crate) struct Argument {
        pub(crate) name: String,
        pub(crate) type_name: String,
        pub(crate) value: Vec<u8>,
    }

    #[derive(Clone)]
    pub(crate) struct RequestBudget {
        pub(crate) cpu_micros: Option<u64>,
        pub(crate) memory_bytes: Option<u64>,
        pub(crate) io_bytes: Option<u64>,
        pub(crate) priority_class: Option<u32>,
    }
}

pub(crate) mod invocation_response {
    use super::{ErrorEnvelope, RpcBatch, RpcCompletion, RpcMetadata};

    #[derive(Clone)]
    pub(crate) enum Response {
        Metadata(RpcMetadata),
        Batch(RpcBatch),
        Completion(RpcCompletion),
        Error(ErrorEnvelope),
    }
}

#[derive(Clone)]
pub(crate) struct ColumnDescriptor {
    pub(crate) name: String,
    pub(crate) ordinal: u32,
    pub(crate) type_name: String,
}

#[derive(Clone)]
pub(crate) struct ResultStreamDescriptor {
    pub(crate) stream_name: String,
    pub(crate) columns: Vec<ColumnDescriptor>,
    pub(crate) cardinality: i32,
    pub(crate) row_count_requirement: i32,
    pub(crate) row_count_exact: Option<u64>,
    pub(crate) row_count_max: Option<u64>,
}

#[derive(Clone)]
pub(crate) struct ResultCompletionPolicy {
    pub(crate) completion_shape: i32,
    pub(crate) reason: String,
}

#[derive(Clone)]
pub(crate) struct RpcMetadata {
    pub(crate) result_streams: Vec<ResultStreamDescriptor>,
    pub(crate) completion_policy: Option<ResultCompletionPolicy>,
}

#[derive(Clone)]
pub(crate) struct RpcBatch {
    pub(crate) result_name: String,
    pub(crate) batch_index: u64,
    pub(crate) rows_emitted: u64,
    pub(crate) structured_payload: Vec<u8>,
    pub(crate) row_count_exact: Option<u64>,
    pub(crate) terminal_batch: bool,
}

#[derive(Clone)]
pub(crate) struct RpcCompletion {
    pub(crate) status: i32,
    pub(crate) rows_affected: Option<u64>,
    pub(crate) tx_id: Option<u64>,
    pub(crate) request_id: Option<u64>,
    pub(crate) session_id: Option<u64>,
    pub(crate) trace_id: Option<String>,
    pub(crate) transaction_outcome: i32,
    pub(crate) durable_lsn: Option<u64>,
    pub(crate) result_row_counts: Vec<rpc_completion::ResultRowCountSummary>,
}

#[derive(Clone)]
pub(crate) struct ErrorEnvelope {
    pub(crate) request_id: Option<u64>,
    pub(crate) session_id: Option<u64>,
    pub(crate) trace_id: Option<String>,
    pub(crate) family: i32,
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) transaction_effect: i32,
    pub(crate) retry_disposition: i32,
    pub(crate) retry_after_ms: Option<u64>,
    pub(crate) backpressure: Option<BackpressureMetadata>,
}

#[derive(Clone)]
pub(crate) struct BackpressureMetadata {
    pub(crate) retry_after_ms: Option<u64>,
    pub(crate) capacity_percent: Option<u32>,
}

#[derive(Clone)]
pub(crate) struct RpcExecuteRequest {
    pub(crate) procedure_name: String,
    pub(crate) expected_contract_hash: Vec<u8>,
    pub(crate) expected_catalog_version: u64,
    pub(crate) surface_scope: String,
    pub(crate) arguments: Vec<rpc_execute_request::Argument>,
    pub(crate) budget: Option<rpc_execute_request::RequestBudget>,
    pub(crate) expected_stats_version: Option<u64>,
}

#[derive(Clone)]
pub(crate) struct InvocationCorrelation {
    pub(crate) request_id: Option<u64>,
    pub(crate) session_id: Option<u64>,
    pub(crate) trace_id: Option<String>,
    pub(crate) contract_hash: Option<Vec<u8>>,
    pub(crate) catalog_version: Option<u64>,
    pub(crate) invocation_id: Option<u64>,
    pub(crate) stats_version: Option<u64>,
    pub(crate) expected_policy_version: Option<u64>,
}

#[derive(Clone)]
pub(crate) struct InvocationRequest {
    pub(crate) correlation: Option<InvocationCorrelation>,
    pub(crate) execute_request: Option<RpcExecuteRequest>,
}

#[derive(Clone)]
pub(crate) struct InvocationResponse {
    pub(crate) correlation: Option<InvocationCorrelation>,
    pub(crate) response_index: Option<u64>,
    pub(crate) response: Option<invocation_response::Response>,
}

pub(crate) fn hash(byte: u8) -> Vec<u8> {
    vec![byte; ContractHash::LEN]
}

pub(crate) fn valid_execute_request() -> RpcExecuteRequest {
    RpcExecuteRequest {
        procedure_name: "Inventory.ReserveStock".to_string(),
        expected_contract_hash: hash(0x11),
        expected_catalog_version: 7,
        surface_scope: "application".to_string(),
        arguments: Vec::new(),
        budget: None,
        expected_stats_version: Some(5),
    }
}

pub(crate) fn valid_correlation() -> InvocationCorrelation {
    InvocationCorrelation {
        request_id: Some(101),
        session_id: Some(202),
        trace_id: Some("trace-proto-101".to_string()),
        contract_hash: Some(hash(0x11)),
        catalog_version: Some(7),
        invocation_id: Some(303),
        stats_version: Some(5),
        expected_policy_version: Some(11),
    }
}

pub(crate) fn valid_invocation_request() -> InvocationRequest {
    InvocationRequest {
        correlation: Some(valid_correlation()),
        execute_request: Some(valid_execute_request()),
    }
}

pub(crate) fn valid_completion() -> RpcCompletion {
    RpcCompletion {
        status: rpc_completion::Status::Committed as i32,
        rows_affected: Some(1),
        tx_id: Some(404),
        request_id: Some(101),
        session_id: Some(202),
        trace_id: Some("trace-proto-101".to_string()),
        transaction_outcome: rpc_completion::TransactionOutcome::Committed as i32,
        durable_lsn: Some(505),
        result_row_counts: vec![rpc_completion::ResultRowCountSummary {
            result_name: "Inventory.ReserveStock.Reservation".to_string(),
            rows_emitted: 1,
            row_count_exact: Some(1),
        }],
    }
}

pub(crate) fn valid_metadata() -> RpcMetadata {
    RpcMetadata {
        result_streams: vec![ResultStreamDescriptor {
            stream_name: "Inventory.ReserveStock.Reservation".to_string(),
            columns: vec![ColumnDescriptor {
                name: "reservation_id".to_string(),
                ordinal: 0,
                type_name: "u64".to_string(),
            }],
            cardinality: result_stream_descriptor::Cardinality::ExactlyOne as i32,
            row_count_requirement: result_stream_descriptor::RowCountRequirement::ExactRequired
                as i32,
            row_count_exact: Some(1),
            row_count_max: Some(1),
        }],
        completion_policy: Some(ResultCompletionPolicy {
            completion_shape: result_completion_policy::CompletionShape::RequiresRowBatch as i32,
            reason: "reservation row required".to_string(),
        }),
    }
}

pub(crate) fn zero_row_metadata() -> RpcMetadata {
    RpcMetadata {
        result_streams: vec![ResultStreamDescriptor {
            stream_name: "Inventory.ReserveStock.EmptyReservation".to_string(),
            columns: vec![ColumnDescriptor {
                name: "reservation_id".to_string(),
                ordinal: 0,
                type_name: "u64".to_string(),
            }],
            cardinality: result_stream_descriptor::Cardinality::ZeroOrMore as i32,
            row_count_requirement: result_stream_descriptor::RowCountRequirement::ExactRequired
                as i32,
            row_count_exact: Some(0),
            row_count_max: Some(0),
        }],
        completion_policy: Some(ResultCompletionPolicy {
            completion_shape: result_completion_policy::CompletionShape::AllowsZeroRowCompletion
                as i32,
            reason: "empty result allowed".to_string(),
        }),
    }
}

pub(crate) fn mutation_only_metadata() -> RpcMetadata {
    RpcMetadata {
        result_streams: Vec::new(),
        completion_policy: Some(ResultCompletionPolicy {
            completion_shape: result_completion_policy::CompletionShape::MutationOnly as i32,
            reason: "mutation only".to_string(),
        }),
    }
}

pub(crate) fn valid_batch(batch_index: u64) -> RpcBatch {
    RpcBatch {
        result_name: "Inventory.ReserveStock.Reservation".to_string(),
        batch_index,
        rows_emitted: 1,
        structured_payload: vec![0xAA],
        row_count_exact: Some(1),
        terminal_batch: true,
    }
}

pub(crate) fn zero_row_completion() -> RpcCompletion {
    RpcCompletion {
        result_row_counts: vec![rpc_completion::ResultRowCountSummary {
            result_name: "Inventory.ReserveStock.EmptyReservation".to_string(),
            rows_emitted: 0,
            row_count_exact: Some(0),
        }],
        ..valid_completion()
    }
}

pub(crate) fn mutation_only_completion() -> RpcCompletion {
    RpcCompletion {
        result_row_counts: Vec::new(),
        ..valid_completion()
    }
}

pub(crate) fn response(
    response_index: u64,
    response: invocation_response::Response,
) -> InvocationResponse {
    InvocationResponse {
        correlation: Some(valid_correlation()),
        response_index: Some(response_index),
        response: Some(response),
    }
}

impl GeneratedColumnDescriptorView for ColumnDescriptor {
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

impl GeneratedResultStreamDescriptorView for ResultStreamDescriptor {
    type Column = ColumnDescriptor;

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

impl GeneratedResultCompletionPolicyView for ResultCompletionPolicy {
    fn completion_shape(&self) -> i32 {
        self.completion_shape
    }
}

impl GeneratedRpcMetadataView for RpcMetadata {
    type ResultStream = ResultStreamDescriptor;
    type CompletionPolicy = ResultCompletionPolicy;

    fn result_streams(&self) -> &[Self::ResultStream] {
        &self.result_streams
    }

    fn completion_policy(&self) -> Option<&Self::CompletionPolicy> {
        self.completion_policy.as_ref()
    }
}

impl GeneratedRpcBatchView for RpcBatch {
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

impl GeneratedResultRowCountSummaryView for rpc_completion::ResultRowCountSummary {
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

impl GeneratedRpcCompletionView for RpcCompletion {
    type ResultRowCountSummary = rpc_completion::ResultRowCountSummary;

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

impl GeneratedBackpressureMetadataView for BackpressureMetadata {
    fn retry_after_ms(&self) -> Option<u64> {
        self.retry_after_ms
    }

    fn capacity_percent(&self) -> Option<u32> {
        self.capacity_percent
    }
}

impl GeneratedErrorEnvelopeView for ErrorEnvelope {
    type Backpressure = BackpressureMetadata;

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

impl GeneratedRpcExecuteArgumentView for rpc_execute_request::Argument {
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

impl GeneratedRpcExecuteRequestBudgetView for rpc_execute_request::RequestBudget {
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

impl GeneratedRpcExecuteRequestView for RpcExecuteRequest {
    type Argument = rpc_execute_request::Argument;
    type Budget = rpc_execute_request::RequestBudget;

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

impl GeneratedInvocationCorrelationView for InvocationCorrelation {
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

impl GeneratedInvocationRequestView for InvocationRequest {
    type Correlation = InvocationCorrelation;
    type ExecuteRequest = RpcExecuteRequest;

    fn correlation(&self) -> Option<&Self::Correlation> {
        self.correlation.as_ref()
    }

    fn execute_request(&self) -> Option<&Self::ExecuteRequest> {
        self.execute_request.as_ref()
    }
}

impl GeneratedInvocationResponseView for InvocationResponse {
    type Correlation = InvocationCorrelation;
    type Metadata = RpcMetadata;
    type Batch = RpcBatch;
    type Completion = RpcCompletion;
    type Error = ErrorEnvelope;

    fn correlation(&self) -> Option<&Self::Correlation> {
        self.correlation.as_ref()
    }

    fn response_index(&self) -> Option<u64> {
        self.response_index
    }

    fn payload(
        &self,
    ) -> Option<
        GeneratedInvocationResponsePayload<
            '_,
            Self::Metadata,
            Self::Batch,
            Self::Completion,
            Self::Error,
        >,
    > {
        match self.response.as_ref()? {
            invocation_response::Response::Metadata(metadata) => {
                Some(GeneratedInvocationResponsePayload::Metadata(metadata))
            },
            invocation_response::Response::Batch(batch) => {
                Some(GeneratedInvocationResponsePayload::Batch(batch))
            },
            invocation_response::Response::Completion(completion) => {
                Some(GeneratedInvocationResponsePayload::Completion(completion))
            },
            invocation_response::Response::Error(error) => {
                Some(GeneratedInvocationResponsePayload::Error(error))
            },
        }
    }
}
