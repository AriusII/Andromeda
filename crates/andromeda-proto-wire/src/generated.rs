#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct GeneratedProtocolVersion {
    #[prost(uint32, tag = "1")]
    pub major: u32,
    #[prost(uint32, tag = "2")]
    pub minor: u32,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct GeneratedFrameEnvelope {
    #[prost(message, optional, tag = "1")]
    pub protocol_version: Option<GeneratedProtocolVersion>,
    #[prost(bytes = "vec", tag = "2")]
    pub contract_hash: Vec<u8>,
    #[prost(uint64, tag = "3")]
    pub catalog_version: u64,
    #[prost(uint64, tag = "4")]
    pub request_id: u64,
    #[prost(uint64, tag = "5")]
    pub session_id: u64,
    #[prost(uint64, optional, tag = "6")]
    pub tx_id: Option<u64>,
    #[prost(enumeration = "GeneratedPayloadKind", tag = "7")]
    pub payload_kind: i32,
    #[prost(bytes = "vec", tag = "8")]
    pub payload: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum GeneratedPayloadKind {
    Unspecified = 0,
    Hello = 1,
    Auth = 2,
    ContractRequest = 3,
    ContractResponse = 4,
    RpcExecuteRequest = 5,
    RpcMetadata = 6,
    RpcBatch = 7,
    RpcCompletion = 8,
    Error = 9,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct GeneratedRpcMetadata {
    #[prost(message, repeated, tag = "1")]
    pub result_streams: Vec<GeneratedResultStreamDescriptor>,
    #[prost(message, optional, tag = "2")]
    pub completion_policy: Option<GeneratedResultCompletionPolicy>,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct GeneratedResultStreamDescriptor {
    #[prost(string, tag = "1")]
    pub stream_name: String,
    #[prost(message, repeated, tag = "2")]
    pub columns: Vec<GeneratedColumnDescriptor>,
    #[prost(enumeration = "GeneratedResultStreamCardinality", tag = "3")]
    pub cardinality: i32,
    #[prost(enumeration = "GeneratedRowCountRequirement", tag = "4")]
    pub row_count_requirement: i32,
    #[prost(uint64, optional, tag = "5")]
    pub row_count_exact: Option<u64>,
    #[prost(uint64, optional, tag = "6")]
    pub row_count_max: Option<u64>,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct GeneratedColumnDescriptor {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(uint32, tag = "2")]
    pub ordinal: u32,
    #[prost(string, tag = "3")]
    pub type_name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum GeneratedResultStreamCardinality {
    Unspecified = 0,
    ZeroOrMore = 1,
    ZeroOrOne = 2,
    OneOrMore = 3,
    ExactlyOne = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum GeneratedRowCountRequirement {
    Unspecified = 0,
    UnknownAllowed = 1,
    ExactIfKnown = 2,
    ExactRequired = 3,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct GeneratedResultCompletionPolicy {
    #[prost(enumeration = "GeneratedCompletionShape", tag = "1")]
    pub completion_shape: i32,
    #[prost(string, tag = "2")]
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum GeneratedCompletionShape {
    Unspecified = 0,
    RequiresRowBatch = 1,
    AllowsZeroRowCompletion = 2,
    MutationOnly = 3,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct GeneratedRpcBatch {
    #[prost(string, tag = "1")]
    pub result_name: String,
    #[prost(uint64, tag = "2")]
    pub batch_index: u64,
    #[prost(uint64, tag = "3")]
    pub rows_emitted: u64,
    #[prost(bytes = "vec", tag = "4")]
    pub structured_payload: Vec<u8>,
    #[prost(uint64, optional, tag = "5")]
    pub row_count_exact: Option<u64>,
    #[prost(bool, tag = "6")]
    pub terminal_batch: bool,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct GeneratedRpcCompletion {
    #[prost(enumeration = "GeneratedRpcCompletionStatus", tag = "1")]
    pub status: i32,
    #[prost(uint64, optional, tag = "2")]
    pub rows_affected: Option<u64>,
    #[prost(uint64, optional, tag = "3")]
    pub tx_id: Option<u64>,
    #[prost(uint64, optional, tag = "32")]
    pub request_id: Option<u64>,
    #[prost(uint64, optional, tag = "33")]
    pub session_id: Option<u64>,
    #[prost(string, optional, tag = "34")]
    pub trace_id: Option<String>,
    #[prost(enumeration = "GeneratedTransactionOutcome", tag = "35")]
    pub transaction_outcome: i32,
    #[prost(uint64, optional, tag = "36")]
    pub durable_lsn: Option<u64>,
    #[prost(message, repeated, tag = "37")]
    pub result_row_counts: Vec<GeneratedResultRowCountSummary>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum GeneratedRpcCompletionStatus {
    Unspecified = 0,
    Committed = 1,
    RolledBack = 2,
    FailedBeforeTransaction = 3,
    Cancelled = 4,
    Poisoned = 5,
    PermissionDenied = 6,
    ContractRejected = 7,
    SystemUnavailable = 8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum GeneratedTransactionOutcome {
    Unspecified = 0,
    NotStarted = 1,
    Committed = 2,
    RolledBack = 3,
    Failed = 4,
    Cancelled = 5,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct GeneratedResultRowCountSummary {
    #[prost(string, tag = "1")]
    pub result_name: String,
    #[prost(uint64, tag = "2")]
    pub rows_emitted: u64,
    #[prost(uint64, optional, tag = "3")]
    pub row_count_exact: Option<u64>,
}
