use andromeda_core::AndromedaResult;

use crate::generated::protocol;
use crate::generated_validation::{contract_error, validate_required_contract_hash};

pub(super) fn validate_required_invocation_correlation(
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

pub(super) fn validate_invocation_policy_version_required(
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

pub(super) fn invocation_correlation_matches(
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

pub(super) fn validate_response_correlation_ids(
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

fn validate_required_nonzero(label: &str, field: &str, value: Option<u64>) -> AndromedaResult<()> {
    match value {
        Some(value) if value != 0 => Ok(()),
        Some(_) => contract_error(format!("{label} {field} must be nonzero")),
        None => contract_error(format!("{label} requires {field}")),
    }
}

pub(super) fn validate_optional_nonzero(label: &str, value: Option<u64>) -> AndromedaResult<()> {
    if matches!(value, Some(0)) {
        return contract_error(format!("{label} must be nonzero when present"));
    }

    Ok(())
}
