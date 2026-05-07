use std::collections::{BTreeMap, BTreeSet};

use andromeda_core::{
    AndromedaErrorKind, AndromedaResult, CatalogVersion, ContractHash, RequestId, SessionId,
    TransactionId,
};

use crate::generated::protocol;
use crate::{
    BackpressureMetadata, ErrorEnvelope, ErrorFamily, FrameEnvelope, PayloadKind, ProtocolVersion,
    RetryDisposition, TransactionEffect,
};

use super::{
    contract_error, protocol_error, validate_generated_result_streams,
    validate_generated_rpc_completion, validate_optional_catalog_version,
    validate_optional_contract_hash, validate_required_contract_hash,
};

const MAX_GENERATED_RPC_EXECUTE_ARGUMENTS: usize = 128;
const MAX_GENERATED_RPC_ARGUMENT_VALUE_BYTES: usize = 16 * 1024 * 1024;
const MAX_GENERATED_INVOCATION_RESPONSE_FRAMES: usize = 1_024;
const MAX_GENERATED_INVOCATION_RESPONSE_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;

pub fn project_generated_frame_envelope(
    envelope: &protocol::v1::FrameEnvelope,
) -> AndromedaResult<FrameEnvelope> {
    let Some(version) = envelope.protocol_version.as_ref() else {
        return protocol_error("generated frame envelope requires protocol_version");
    };

    let payload_kind = project_payload_kind(envelope.payload_kind)?;
    let contract_hash = project_contract_hash_for_payload_kind(
        "generated frame envelope contract_hash",
        payload_kind,
        &envelope.contract_hash,
    )?;

    FrameEnvelope {
        protocol_version: ProtocolVersion {
            major: version.major,
            minor: version.minor,
        },
        contract_hash,
        catalog_version: CatalogVersion::new(envelope.catalog_version),
        request_id: RequestId::new(envelope.request_id),
        session_id: SessionId::new(envelope.session_id),
        tx_id: envelope.tx_id.map(TransactionId::new),
        payload_kind,
        payload: envelope.payload.clone(),
    }
    .validated()
}

pub fn validate_generated_frame_envelope(
    envelope: &protocol::v1::FrameEnvelope,
) -> AndromedaResult<()> {
    project_generated_frame_envelope(envelope).map(|_| ())
}

pub fn validate_generated_rpc_execute_request(
    request: &protocol::v1::RpcExecuteRequest,
) -> AndromedaResult<()> {
    if request.procedure_name.trim().is_empty() {
        return contract_error("generated RPC execute request procedure_name must be non-empty");
    }

    validate_required_contract_hash(
        "generated RPC execute request expected_contract_hash",
        &request.expected_contract_hash,
    )?;

    validate_optional_catalog_version(
        "generated RPC execute request expected_catalog_version",
        Some(request.expected_catalog_version),
    )?;

    match request.expected_stats_version {
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

    if request.surface_scope.trim().is_empty() {
        return contract_error("generated RPC execute request surface_scope must be non-empty");
    }

    validate_rpc_execute_arguments(&request.arguments)?;
    validate_rpc_execute_budget(request.budget.as_ref())?;

    Ok(())
}

pub fn validate_generated_rpc_metadata(
    metadata: &protocol::v1::RpcMetadata,
) -> AndromedaResult<()> {
    validate_generated_result_streams(&metadata.result_streams)?;

    let Some(policy) = metadata.completion_policy.as_ref() else {
        return contract_error("generated RPC metadata requires completion_policy");
    };

    validate_result_completion_policy(policy)
}

pub fn validate_generated_rpc_batch(batch: &protocol::v1::RpcBatch) -> AndromedaResult<()> {
    if batch.result_name.trim().is_empty() {
        return contract_error("generated RPC batch result_name must be non-empty");
    }

    if batch.structured_payload.is_empty() {
        return protocol_error("generated RPC batch structured_payload must be non-empty");
    }

    if batch.rows_emitted == 0 {
        return contract_error("generated RPC batch rows_emitted must be nonzero");
    }

    if let Some(exact) = batch.row_count_exact
        && exact < batch.rows_emitted
    {
        return contract_error("generated RPC batch row_count_exact is below rows_emitted");
    }

    Ok(())
}

pub fn validate_generated_invocation_request(
    request: &protocol::v1::InvocationRequest,
) -> AndromedaResult<()> {
    let Some(correlation) = request.correlation.as_ref() else {
        return contract_error("generated invocation request requires correlation");
    };
    let Some(execute_request) = request.execute_request.as_ref() else {
        return contract_error("generated invocation request requires execute_request");
    };

    validate_generated_rpc_execute_request(execute_request)?;
    validate_required_invocation_correlation(
        "generated invocation request correlation",
        correlation,
    )?;

    let Some(correlation_contract_hash) = correlation.contract_hash.as_deref() else {
        return contract_error("generated invocation request correlation requires contract_hash");
    };
    if correlation_contract_hash != execute_request.expected_contract_hash.as_slice() {
        return contract_error(
            "generated invocation request correlation contract_hash must match execute request",
        );
    }

    if correlation.catalog_version != Some(execute_request.expected_catalog_version) {
        return contract_error(
            "generated invocation request correlation catalog_version must match execute request",
        );
    }

    if correlation.stats_version != execute_request.expected_stats_version {
        return contract_error(
            "generated invocation request correlation stats_version must match execute request",
        );
    }

    validate_invocation_policy_version_required(
        "generated invocation request security context",
        correlation.expected_policy_version,
    )?;

    Ok(())
}

pub fn validate_generated_invocation_response(
    response: &protocol::v1::InvocationResponse,
) -> AndromedaResult<()> {
    let Some(correlation) = response.correlation.as_ref() else {
        return contract_error("generated invocation response requires correlation");
    };
    validate_required_invocation_correlation(
        "generated invocation response correlation",
        correlation,
    )?;

    use protocol::v1::invocation_response::Response;
    match response.response.as_ref() {
        Some(Response::Metadata(metadata)) => validate_generated_rpc_metadata(metadata),
        Some(Response::Batch(batch)) => validate_generated_rpc_batch(batch),
        Some(Response::Completion(completion)) => {
            validate_generated_rpc_completion(completion)?;
            validate_response_correlation_ids(
                "generated invocation completion",
                correlation,
                completion.request_id,
                completion.session_id,
            )
        }
        Some(Response::Error(error)) => {
            validate_generated_error_envelope(error)?;
            validate_response_correlation_ids(
                "generated invocation error",
                correlation,
                error.request_id,
                error.session_id,
            )
        }
        None => contract_error("generated invocation response requires a typed response payload"),
    }
}

pub fn validate_generated_invocation_response_sequence(
    responses: &[protocol::v1::InvocationResponse],
) -> AndromedaResult<()> {
    if responses.is_empty() {
        return contract_error("generated invocation response sequence must be non-empty");
    }
    if responses.len() > MAX_GENERATED_INVOCATION_RESPONSE_FRAMES {
        return protocol_error(format!(
            "generated invocation response sequence frame count exceeds bounded limit of {MAX_GENERATED_INVOCATION_RESPONSE_FRAMES}"
        ));
    }

    let mut state = InvocationResponseSequenceState::default();
    for (ordinal, response) in responses.iter().enumerate() {
        validate_generated_invocation_response(response)?;
        state.observe(ordinal, response)?;
    }

    state.finish()
}

pub fn validate_generated_error_envelope(
    error: &protocol::v1::ErrorEnvelope,
) -> AndromedaResult<()> {
    let backpressure = error
        .backpressure
        .as_ref()
        .map(project_backpressure_metadata)
        .transpose()?;

    ErrorEnvelope {
        request_id: error.request_id.map(RequestId::new),
        session_id: error.session_id.map(SessionId::new),
        trace_id: error.trace_id.clone(),
        family: project_error_family(error.family)?,
        code: error.code.clone(),
        message: error.message.clone(),
        transaction_effect: project_transaction_effect(error.transaction_effect)?,
        retry_disposition: project_retry_disposition(error.retry_disposition)?,
        retry_after_ms: error.retry_after_ms,
        backpressure,
    }
    .validate()
}

fn validate_rpc_execute_arguments(
    arguments: &[protocol::v1::rpc_execute_request::Argument],
) -> AndromedaResult<()> {
    if arguments.len() > MAX_GENERATED_RPC_EXECUTE_ARGUMENTS {
        return contract_error(format!(
            "generated RPC execute request arguments exceed bounded limit of {MAX_GENERATED_RPC_EXECUTE_ARGUMENTS}"
        ));
    }

    let mut seen_arguments = BTreeSet::new();
    for argument in arguments {
        if argument.name.trim().is_empty() {
            return contract_error("generated RPC execute argument name must be non-empty");
        }
        if argument.type_name.trim().is_empty() {
            return contract_error("generated RPC execute argument type_name must be non-empty");
        }
        if argument.value.is_empty() {
            return contract_error("generated RPC execute argument value must be non-empty binary");
        }
        if argument.value.len() > MAX_GENERATED_RPC_ARGUMENT_VALUE_BYTES {
            return contract_error(format!(
                "generated RPC execute argument value exceeds bounded limit of {MAX_GENERATED_RPC_ARGUMENT_VALUE_BYTES} bytes"
            ));
        }
        if !seen_arguments.insert(argument.name.clone()) {
            return contract_error("generated RPC execute argument names must be unique");
        }
    }

    Ok(())
}

fn validate_rpc_execute_budget(
    budget: Option<&protocol::v1::rpc_execute_request::RequestBudget>,
) -> AndromedaResult<()> {
    let Some(budget) = budget else {
        return Ok(());
    };

    validate_optional_nonzero("generated RPC execute budget cpu_micros", budget.cpu_micros)?;
    validate_optional_nonzero(
        "generated RPC execute budget memory_bytes",
        budget.memory_bytes,
    )?;
    validate_optional_nonzero("generated RPC execute budget io_bytes", budget.io_bytes)?;

    if matches!(budget.priority_class, Some(0)) {
        return contract_error("generated RPC execute budget priority_class must be nonzero");
    }

    Ok(())
}

#[derive(Default)]
struct InvocationResponseSequenceState {
    correlation: Option<protocol::v1::InvocationCorrelation>,
    metadata_seen: bool,
    batch_seen: bool,
    terminal_seen: bool,
    completion_shape: Option<i32>,
    total_structured_payload_bytes: u64,
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
    fn observe(
        &mut self,
        ordinal: usize,
        response: &protocol::v1::InvocationResponse,
    ) -> AndromedaResult<()> {
        let Some(correlation) = response.correlation.as_ref() else {
            return contract_error("generated invocation response sequence requires correlation");
        };
        self.observe_correlation(correlation)?;
        self.validate_response_index(ordinal, response.response_index)?;

        if self.terminal_seen {
            return contract_error(
                "generated invocation response sequence has payload after terminal response",
            );
        }

        use protocol::v1::invocation_response::Response;
        match response.response.as_ref() {
            Some(Response::Metadata(metadata)) => self.observe_metadata(metadata),
            Some(Response::Batch(batch)) => self.observe_batch(batch),
            Some(Response::Completion(completion)) => self.observe_completion(completion),
            Some(Response::Error(_)) => self.observe_error(),
            None => contract_error(
                "generated invocation response sequence requires typed response payload",
            ),
        }
    }

    fn observe_correlation(
        &mut self,
        correlation: &protocol::v1::InvocationCorrelation,
    ) -> AndromedaResult<()> {
        if let Some(expected) = self.correlation.as_ref() {
            if !invocation_correlation_matches(expected, correlation) {
                return contract_error(
                    "generated invocation response sequence correlation must remain stable",
                );
            }
        } else {
            self.correlation = Some(correlation.clone());
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

    fn observe_metadata(&mut self, metadata: &protocol::v1::RpcMetadata) -> AndromedaResult<()> {
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
        let Some(policy) = metadata.completion_policy.as_ref() else {
            return contract_error(
                "generated invocation response sequence metadata requires completion_policy",
            );
        };
        self.completion_shape = Some(policy.completion_shape);

        if policy.completion_shape
            == protocol::v1::result_completion_policy::CompletionShape::MutationOnly as i32
            && !metadata.result_streams.is_empty()
        {
            return contract_error(
                "generated invocation response sequence mutation-only metadata must not declare result streams",
            );
        }

        for descriptor in &metadata.result_streams {
            self.declared_streams.insert(
                descriptor.stream_name.clone(),
                StreamSequenceState {
                    next_batch_index: 0,
                    rows_emitted: 0,
                    row_count_exact: descriptor.row_count_exact,
                    row_count_max: descriptor.row_count_max,
                    terminal_batch_seen: false,
                },
            );
        }

        Ok(())
    }

    fn observe_batch(&mut self, batch: &protocol::v1::RpcBatch) -> AndromedaResult<()> {
        if !self.metadata_seen {
            return contract_error(
                "generated invocation response sequence batch must follow metadata",
            );
        }

        let Some(stream) = self.declared_streams.get_mut(&batch.result_name) else {
            return contract_error(
                "generated invocation response sequence batch result_name must be declared by metadata",
            );
        };

        if stream.terminal_batch_seen {
            return contract_error(
                "generated invocation response sequence has batch after terminal batch",
            );
        }

        if batch.batch_index != stream.next_batch_index {
            return contract_error(
                "generated invocation response sequence batch_index must be gap-free per result stream",
            );
        }
        stream.next_batch_index = stream.next_batch_index.checked_add(1).ok_or_else(|| {
            andromeda_core::AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "generated invocation response sequence batch_index overflow",
            )
        })?;

        stream.rows_emitted = stream
            .rows_emitted
            .checked_add(batch.rows_emitted)
            .ok_or_else(|| {
                andromeda_core::AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    "generated invocation response sequence rows_emitted overflow",
                )
            })?;

        let payload_len = u64::try_from(batch.structured_payload.len()).map_err(|_| {
            andromeda_core::AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "generated invocation response sequence structured_payload length does not fit u64",
            )
        })?;
        self.total_structured_payload_bytes = self
            .total_structured_payload_bytes
            .checked_add(payload_len)
            .ok_or_else(|| {
                andromeda_core::AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    "generated invocation response sequence structured_payload byte count overflow",
                )
            })?;
        if self.total_structured_payload_bytes > MAX_GENERATED_INVOCATION_RESPONSE_PAYLOAD_BYTES {
            return protocol_error(format!(
                "generated invocation response sequence structured_payload bytes exceed bounded limit of {MAX_GENERATED_INVOCATION_RESPONSE_PAYLOAD_BYTES}"
            ));
        }

        if let Some(expected_exact) = stream.row_count_exact {
            if batch.row_count_exact != Some(expected_exact) {
                return contract_error(
                    "generated invocation response sequence batch row_count_exact must match metadata",
                );
            }
            if stream.rows_emitted > expected_exact {
                return contract_error(
                    "generated invocation response sequence rows_emitted exceeds row_count_exact",
                );
            }
        } else if batch.row_count_exact.is_some() {
            return contract_error(
                "generated invocation response sequence batch must not introduce row_count_exact absent from metadata",
            );
        }

        if let Some(max) = stream.row_count_max
            && stream.rows_emitted > max
        {
            return contract_error(
                "generated invocation response sequence rows_emitted exceeds row_count_max",
            );
        }

        if batch.terminal_batch {
            stream.terminal_batch_seen = true;
        }
        self.batch_seen = true;

        Ok(())
    }

    fn observe_completion(
        &mut self,
        completion: &protocol::v1::RpcCompletion,
    ) -> AndromedaResult<()> {
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

    fn validate_completion_row_counts(
        &self,
        completion: &protocol::v1::RpcCompletion,
    ) -> AndromedaResult<()> {
        let mut seen_summaries = BTreeSet::new();
        for summary in &completion.result_row_counts {
            if !seen_summaries.insert(summary.result_name.clone()) {
                return contract_error(
                    "generated invocation response completion result summaries must be unique",
                );
            }

            let Some(stream) = self.declared_streams.get(&summary.result_name) else {
                return contract_error(
                    "generated invocation response completion result_name must be declared by metadata",
                );
            };

            if summary.rows_emitted != stream.rows_emitted {
                return contract_error(
                    "generated invocation response completion rows_emitted must match batches",
                );
            }

            if let Some(expected_exact) = stream.row_count_exact
                && summary.row_count_exact != Some(expected_exact)
            {
                return contract_error(
                    "generated invocation response completion row_count_exact must match metadata",
                );
            }
            if stream.row_count_exact.is_none() && summary.row_count_exact.is_some() {
                return contract_error(
                    "generated invocation response completion must not introduce row_count_exact absent from metadata",
                );
            }
        }

        for (result_name, stream) in &self.declared_streams {
            if !seen_summaries.contains(result_name) {
                return contract_error(
                    "generated invocation response completion must summarize every declared result stream",
                );
            }

            if stream.rows_emitted > 0 && !stream.terminal_batch_seen {
                return contract_error(
                    "generated invocation response completion requires terminal batch before completion",
                );
            }

            if let Some(expected_exact) = stream.row_count_exact
                && stream.rows_emitted != expected_exact
            {
                return contract_error(
                    "generated invocation response sequence rows_emitted must equal row_count_exact before completion",
                );
            }
        }

        Ok(())
    }

    fn validate_completion_policy(&self) -> AndromedaResult<()> {
        use protocol::v1::result_completion_policy::CompletionShape;

        match self.completion_shape {
            Some(value) if value == CompletionShape::RequiresRowBatch as i32 => {
                if self.batch_seen {
                    Ok(())
                } else {
                    contract_error(
                        "generated invocation response sequence requires row batch before completion",
                    )
                }
            }
            Some(value) if value == CompletionShape::AllowsZeroRowCompletion as i32 => Ok(()),
            Some(value) if value == CompletionShape::MutationOnly as i32 => {
                if self.batch_seen || !self.declared_streams.is_empty() {
                    contract_error(
                        "generated invocation response sequence mutation-only completion must not include row streams",
                    )
                } else {
                    Ok(())
                }
            }
            Some(value) if value == CompletionShape::Unspecified as i32 => contract_error(
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

fn validate_result_completion_policy(
    policy: &protocol::v1::ResultCompletionPolicy,
) -> AndromedaResult<()> {
    use protocol::v1::result_completion_policy::CompletionShape;

    match policy.completion_shape {
        value if value == CompletionShape::RequiresRowBatch as i32 => Ok(()),
        value if value == CompletionShape::AllowsZeroRowCompletion as i32 => Ok(()),
        value if value == CompletionShape::MutationOnly as i32 => Ok(()),
        value if value == CompletionShape::Unspecified as i32 => {
            contract_error("generated RPC metadata completion_shape must be specified")
        }
        _ => protocol_error("unknown generated RPC metadata completion_shape"),
    }
}

fn validate_required_invocation_correlation(
    label: &str,
    correlation: &protocol::v1::InvocationCorrelation,
) -> AndromedaResult<()> {
    validate_required_nonzero(label, "request_id", correlation.request_id)?;
    validate_required_nonzero(label, "session_id", correlation.session_id)?;

    if matches!(correlation.trace_id.as_deref(), Some(trace_id) if trace_id.trim().is_empty()) {
        return contract_error(format!("{label} trace_id must be non-empty when present"));
    }

    match correlation.contract_hash.as_deref() {
        Some(bytes) => {
            let field_label = format!("{label} contract_hash");
            validate_required_contract_hash(&field_label, bytes)?;
        }
        None => {
            return contract_error(format!("{label} requires contract_hash"));
        }
    }

    match correlation.catalog_version {
        Some(value) if value != 0 => {}
        Some(_) => {
            return contract_error(format!("{label} catalog_version must be nonzero"));
        }
        None => {
            return contract_error(format!("{label} requires catalog_version"));
        }
    }

    let invocation_id_label = format!("{label} invocation_id");
    validate_optional_nonzero(&invocation_id_label, correlation.invocation_id)?;

    match correlation.stats_version {
        Some(value) if value != 0 => {}
        Some(_) => {
            return contract_error(format!("{label} stats_version must be nonzero"));
        }
        None => {
            return contract_error(format!("{label} requires stats_version"));
        }
    }

    validate_invocation_policy_version_required(label, correlation.expected_policy_version)?;

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

fn invocation_correlation_matches(
    expected: &protocol::v1::InvocationCorrelation,
    actual: &protocol::v1::InvocationCorrelation,
) -> bool {
    expected.request_id == actual.request_id
        && expected.session_id == actual.session_id
        && expected.trace_id == actual.trace_id
        && expected.contract_hash == actual.contract_hash
        && expected.catalog_version == actual.catalog_version
        && expected.invocation_id == actual.invocation_id
        && expected.stats_version == actual.stats_version
        && expected.expected_policy_version == actual.expected_policy_version
}

fn validate_response_correlation_ids(
    label: &str,
    correlation: &protocol::v1::InvocationCorrelation,
    payload_request_id: Option<u64>,
    payload_session_id: Option<u64>,
) -> AndromedaResult<()> {
    if payload_request_id != correlation.request_id {
        return contract_error(format!(
            "{label} request_id must match invocation correlation"
        ));
    }

    if payload_session_id != correlation.session_id {
        return contract_error(format!(
            "{label} session_id must match invocation correlation"
        ));
    }

    Ok(())
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

fn project_error_family(error_family: i32) -> AndromedaResult<ErrorFamily> {
    use protocol::v1::error_envelope::ErrorFamily as GeneratedErrorFamily;

    match error_family {
        value if value == GeneratedErrorFamily::Protocol as i32 => Ok(ErrorFamily::Protocol),
        value if value == GeneratedErrorFamily::Authentication as i32 => {
            Ok(ErrorFamily::Authentication)
        }
        value if value == GeneratedErrorFamily::Authorization as i32 => {
            Ok(ErrorFamily::Authorization)
        }
        value if value == GeneratedErrorFamily::Contract as i32 => Ok(ErrorFamily::Contract),
        value if value == GeneratedErrorFamily::Semantic as i32 => Ok(ErrorFamily::Semantic),
        value if value == GeneratedErrorFamily::Execution as i32 => Ok(ErrorFamily::Execution),
        value if value == GeneratedErrorFamily::Transaction as i32 => Ok(ErrorFamily::Transaction),
        value if value == GeneratedErrorFamily::Storage as i32 => Ok(ErrorFamily::Storage),
        value if value == GeneratedErrorFamily::Resource as i32 => Ok(ErrorFamily::Resource),
        value if value == GeneratedErrorFamily::Unspecified as i32 => {
            protocol_error("generated error family must be specified")
        }
        _ => protocol_error("unknown generated error family"),
    }
}

fn project_transaction_effect(effect: i32) -> AndromedaResult<TransactionEffect> {
    use protocol::v1::error_envelope::TransactionEffect as GeneratedTransactionEffect;

    match effect {
        value if value == GeneratedTransactionEffect::NoTransaction as i32 => {
            Ok(TransactionEffect::NoTransaction)
        }
        value if value == GeneratedTransactionEffect::RollbackRequired as i32 => {
            Ok(TransactionEffect::RollbackRequired)
        }
        value if value == GeneratedTransactionEffect::FailStop as i32 => {
            Ok(TransactionEffect::FailStop)
        }
        value if value == GeneratedTransactionEffect::Unspecified as i32 => {
            protocol_error("generated error transaction_effect must be specified")
        }
        _ => protocol_error("unknown generated error transaction_effect"),
    }
}

fn project_retry_disposition(disposition: i32) -> AndromedaResult<RetryDisposition> {
    use protocol::v1::error_envelope::RetryDisposition as GeneratedRetryDisposition;

    match disposition {
        value if value == GeneratedRetryDisposition::NotRetryable as i32 => {
            Ok(RetryDisposition::NotRetryable)
        }
        value if value == GeneratedRetryDisposition::Retryable as i32 => {
            Ok(RetryDisposition::Retryable)
        }
        value if value == GeneratedRetryDisposition::RetryAfter as i32 => {
            Ok(RetryDisposition::RetryAfter)
        }
        value if value == GeneratedRetryDisposition::Backpressure as i32 => {
            Ok(RetryDisposition::Backpressure)
        }
        value if value == GeneratedRetryDisposition::Unspecified as i32 => {
            protocol_error("generated error retry_disposition must be specified")
        }
        _ => protocol_error("unknown generated error retry_disposition"),
    }
}

fn project_backpressure_metadata(
    backpressure: &protocol::v1::error_envelope::BackpressureMetadata,
) -> AndromedaResult<BackpressureMetadata> {
    let capacity_percent = match backpressure.capacity_percent {
        Some(percent) => Some(u8::try_from(percent).map_err(|_| {
            andromeda_core::AndromedaError::new(
                AndromedaErrorKind::Resource,
                "generated backpressure capacity_percent must fit in u8",
            )
        })?),
        None => None,
    };

    Ok(BackpressureMetadata {
        retry_after_ms: backpressure.retry_after_ms,
        capacity_percent,
        shed_load: backpressure.shed_load,
    })
}
