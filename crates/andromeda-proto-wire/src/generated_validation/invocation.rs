use std::collections::{BTreeMap, BTreeSet};

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{
    StructuredPayloadByteTracker, validate_required_contract_hash,
    validate_rpc_execute_argument_value, validate_rpc_execute_arguments_len,
    validate_rpc_response_sequence_len,
};

use super::{
    common::{contract_error, protocol_error, validate_optional_catalog_version},
    error::validate_generated_error_envelope,
    rpc_result::{
        validate_generated_rpc_batch, validate_generated_rpc_completion,
        validate_generated_rpc_metadata,
    },
    views::{
        GeneratedErrorEnvelopeView, GeneratedInvocationCorrelationView,
        GeneratedInvocationRequestView, GeneratedInvocationResponsePayload,
        GeneratedInvocationResponseView, GeneratedResultCompletionPolicyView,
        GeneratedResultRowCountSummaryView, GeneratedResultStreamDescriptorView,
        GeneratedRpcBatchView, GeneratedRpcCompletionView, GeneratedRpcExecuteArgumentView,
        GeneratedRpcExecuteRequestBudgetView, GeneratedRpcExecuteRequestView,
        GeneratedRpcMetadataView,
    },
};

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
        Some(value) if value != 0 => {},
        Some(_) => {
            return contract_error(
                "generated RPC execute request expected_stats_version must be nonzero",
            );
        },
        None => {
            return contract_error("generated RPC execute request requires expected_stats_version");
        },
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
        },
        Some(GeneratedInvocationResponsePayload::Batch(batch)) => {
            validate_generated_rpc_batch(batch)
        },
        Some(GeneratedInvocationResponsePayload::Completion(completion)) => {
            validate_generated_rpc_completion(completion)?;
            validate_response_correlation_ids(
                "generated invocation completion",
                correlation,
                completion.request_id(),
                completion.session_id(),
            )
        },
        Some(GeneratedInvocationResponsePayload::Error(error)) => {
            validate_generated_error_envelope(error)?;
            validate_response_correlation_ids(
                "generated invocation error",
                correlation,
                error.request_id(),
                error.session_id(),
            )
        },
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
        },
        None => {
            return contract_error(format!("{label} requires contract_hash"));
        },
    }

    match correlation.catalog_version() {
        Some(value) if value != 0 => {},
        Some(_) => {
            return contract_error(format!("{label} catalog_version must be nonzero"));
        },
        None => {
            return contract_error(format!("{label} requires catalog_version"));
        },
    }

    let invocation_id_label = format!("{label} invocation_id");
    validate_optional_nonzero(&invocation_id_label, correlation.invocation_id())?;

    match correlation.stats_version() {
        Some(value) if value != 0 => {},
        Some(_) => {
            return contract_error(format!("{label} stats_version must be nonzero"));
        },
        None => {
            return contract_error(format!("{label} requires stats_version"));
        },
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
            },
            Some(GeneratedInvocationResponsePayload::Batch(batch)) => self.observe_batch(batch),
            Some(GeneratedInvocationResponsePayload::Completion(completion)) => {
                self.observe_completion(completion)
            },
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
            },
            Some(2) => Ok(()),
            Some(3) => {
                if self.batch_seen || !self.declared_streams.is_empty() {
                    contract_error(
                        "generated invocation response sequence mutation-only completion must not include row streams",
                    )
                } else {
                    Ok(())
                }
            },
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
