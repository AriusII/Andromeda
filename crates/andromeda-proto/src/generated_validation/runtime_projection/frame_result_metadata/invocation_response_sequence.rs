use std::collections::{BTreeMap, BTreeSet};

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::generated::protocol;
use crate::generated_validation::{contract_error, protocol_error};

use super::super::invocation_correlation::invocation_correlation_matches;
use super::super::structured_payload_bounds::{
    StructuredPayloadByteTracker, validate_rpc_response_sequence_len,
};
use super::validate_generated_invocation_response;

pub fn validate_generated_invocation_response_sequence(
    responses: &[protocol::v1::InvocationResponse],
) -> AndromedaResult<()> {
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

#[derive(Default)]
struct InvocationResponseSequenceState {
    correlation: Option<protocol::v1::InvocationCorrelation>,
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
                StreamSequenceState::new(descriptor.row_count_exact, descriptor.row_count_max),
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

        stream.observe_batch(batch)?;
        self.structured_payload
            .observe_batch_payload(&batch.structured_payload)?;
        stream.validate_observed_batch_counts(batch)?;
        stream.observe_terminal_batch(batch.terminal_batch);
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

    fn observe_batch(&mut self, batch: &protocol::v1::RpcBatch) -> AndromedaResult<()> {
        if self.terminal_batch_seen {
            return contract_error(
                "generated invocation response sequence has batch after terminal batch",
            );
        }

        if batch.batch_index != self.next_batch_index {
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
            .checked_add(batch.rows_emitted)
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    "generated invocation response sequence rows_emitted overflow",
                )
            })?;

        Ok(())
    }

    fn validate_observed_batch_counts(
        &self,
        batch: &protocol::v1::RpcBatch,
    ) -> AndromedaResult<()> {
        self.validate_batch_row_count_exact(batch.row_count_exact)?;
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

    fn validate_completion_summary(
        &self,
        summary: &protocol::v1::rpc_completion::ResultRowCountSummary,
    ) -> AndromedaResult<()> {
        if summary.rows_emitted != self.rows_emitted {
            return contract_error(
                "generated invocation response completion rows_emitted must match batches",
            );
        }

        if let Some(expected_exact) = self.row_count_exact
            && summary.row_count_exact != Some(expected_exact)
        {
            return contract_error(
                "generated invocation response completion row_count_exact must match metadata",
            );
        }
        if self.row_count_exact.is_none() && summary.row_count_exact.is_some() {
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
