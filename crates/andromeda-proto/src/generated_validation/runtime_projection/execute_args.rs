use std::collections::BTreeSet;

use andromeda_error::AndromedaResult;

use crate::generated::protocol;

use super::super::{
    contract_error, validate_optional_catalog_version, validate_required_contract_hash,
};
use super::invocation_correlation::{
    validate_invocation_policy_version_required, validate_optional_nonzero,
    validate_required_invocation_correlation,
};
use super::structured_payload_bounds::{
    validate_rpc_execute_argument_value, validate_rpc_execute_arguments_len,
};

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

fn validate_rpc_execute_arguments(
    arguments: &[protocol::v1::rpc_execute_request::Argument],
) -> AndromedaResult<()> {
    validate_rpc_execute_arguments_len(arguments.len())?;

    let mut seen_arguments = BTreeSet::new();
    for argument in arguments {
        if argument.name.trim().is_empty() {
            return contract_error("generated RPC execute argument name must be non-empty");
        }
        if argument.type_name.trim().is_empty() {
            return contract_error("generated RPC execute argument type_name must be non-empty");
        }
        validate_rpc_execute_argument_value(&argument.value)?;
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
