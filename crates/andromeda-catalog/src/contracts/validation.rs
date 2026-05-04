//! Compatibility checking between procedure contract versions.

use super::{CompatibilityPolicy, ContractCompatibilityDiagnostic, ProcedureContract};

pub fn diagnose_procedure_contract_compatibility(
    previous: &ProcedureContract,
    next: &ProcedureContract,
) -> ContractCompatibilityDiagnostic {
    let mut messages = Vec::new();

    if let Err(error) = previous.validate() {
        messages.push(format!("previous contract is invalid: {}", error.message()));
    }
    if let Err(error) = next.validate() {
        messages.push(format!("next contract is invalid: {}", error.message()));
    }
    if !messages.is_empty() {
        return ContractCompatibilityDiagnostic::incompatible(messages);
    }

    if previous.object.name != next.object.name {
        messages
            .push("procedure contract compatibility requires the same procedure name".to_string());
    }

    match next.compatibility_policy {
        CompatibilityPolicy::ExactHash => {
            if previous.contract_hash != next.contract_hash {
                messages
                    .push("exact-hash compatibility requires unchanged contract hash".to_string());
            }
        }
        CompatibilityPolicy::AdditiveOnly => {
            if previous.inputs != next.inputs {
                messages.push("additive compatibility does not permit input changes".to_string());
            }
            if previous.structured_inputs != next.structured_inputs {
                messages.push(
                    "additive compatibility does not permit structured input changes".to_string(),
                );
            }
            if previous.required_permissions != next.required_permissions {
                messages
                    .push("additive compatibility does not permit permission changes".to_string());
            }
            if previous.stats_version != next.stats_version {
                messages.push(
                    "additive compatibility does not permit stats version changes".to_string(),
                );
            }
            if previous.protocol_layout != next.protocol_layout {
                messages.push(
                    "additive compatibility does not permit protocol layout changes".to_string(),
                );
            }
            if previous.transaction_policy != next.transaction_policy {
                messages.push(
                    "additive compatibility does not permit transaction policy changes".to_string(),
                );
            }
            if previous.result_metadata_policy != next.result_metadata_policy {
                messages.push(
                    "additive compatibility does not permit result metadata policy changes"
                        .to_string(),
                );
            }
            if previous.error_policy != next.error_policy {
                messages.push(
                    "additive compatibility does not permit error policy changes".to_string(),
                );
            }
            if previous.multi_result_policy != next.multi_result_policy {
                messages.push(
                    "additive compatibility does not permit multi-result policy changes"
                        .to_string(),
                );
            }
            for previous_stream in &previous.result_streams {
                let Some(next_stream) = next
                    .result_streams
                    .iter()
                    .find(|stream| stream.name == previous_stream.name)
                else {
                    messages.push(format!(
                        "additive compatibility does not permit removing result stream {}",
                        previous_stream.name
                    ));
                    continue;
                };

                if previous_stream.stream_id != next_stream.stream_id {
                    messages.push(format!(
                        "additive compatibility does not permit changing result stream id for result stream {}",
                        previous_stream.name
                    ));
                }

                if previous_stream.row_count_exact_required != next_stream.row_count_exact_required
                {
                    messages.push(format!(
                        "additive compatibility does not permit changing cardinality contract for result stream {}",
                        previous_stream.name
                    ));
                }

                if next_stream.columns.len() < previous_stream.columns.len()
                    || !next_stream
                        .columns
                        .iter()
                        .zip(previous_stream.columns.iter())
                        .all(|(next_column, previous_column)| next_column == previous_column)
                {
                    messages.push(format!(
                        "additive compatibility requires existing columns to remain an unchanged prefix for result stream {}",
                        previous_stream.name
                    ));
                }
            }
        }
    }

    if messages.is_empty() {
        ContractCompatibilityDiagnostic::compatible()
    } else {
        ContractCompatibilityDiagnostic::incompatible(messages)
    }
}
