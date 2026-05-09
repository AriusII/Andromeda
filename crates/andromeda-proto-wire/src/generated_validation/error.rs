use andromeda_error::AndromedaResult;

use super::{
    common::{contract_error, protocol_error, resource_error},
    views::{GeneratedBackpressureMetadataView, GeneratedErrorEnvelopeView},
};

pub fn validate_generated_error_envelope<T>(error: &T) -> AndromedaResult<()>
where
    T: GeneratedErrorEnvelopeView,
{
    validate_error_family(error.family())?;
    validate_transaction_effect(error.transaction_effect())?;
    let retry_disposition = validate_retry_disposition(error.retry_disposition())?;

    if error.code().trim().is_empty() {
        return contract_error("error envelope code must not be empty");
    }

    if error.message().trim().is_empty() {
        return contract_error("error envelope message must not be empty");
    }

    if matches!(error.trace_id(), Some(trace_id) if trace_id.trim().is_empty()) {
        return contract_error("error trace correlation id must not be empty when present");
    }

    if let Some(backpressure) = error.backpressure() {
        validate_backpressure_metadata(backpressure)?;
    }

    if retry_disposition == GeneratedRetryDispositionCode::RetryAfter
        && error.retry_after_ms().is_none()
        && error
            .backpressure()
            .and_then(GeneratedBackpressureMetadataView::retry_after_ms)
            .is_none()
    {
        return resource_error("retry-after error must include retry delay metadata");
    }

    if retry_disposition == GeneratedRetryDispositionCode::Backpressure
        && error.backpressure().is_none()
    {
        return resource_error("backpressure error must include backpressure metadata");
    }

    Ok(())
}

fn validate_error_family(error_family: i32) -> AndromedaResult<()> {
    match error_family {
        1..=9 => Ok(()),
        0 => protocol_error("generated error family must be specified"),
        _ => protocol_error("unknown generated error family"),
    }
}

fn validate_transaction_effect(effect: i32) -> AndromedaResult<()> {
    match effect {
        1..=3 => Ok(()),
        0 => protocol_error("generated error transaction_effect must be specified"),
        _ => protocol_error("unknown generated error transaction_effect"),
    }
}

fn validate_retry_disposition(disposition: i32) -> AndromedaResult<GeneratedRetryDispositionCode> {
    match disposition {
        1 => Ok(GeneratedRetryDispositionCode::NotRetryable),
        2 => Ok(GeneratedRetryDispositionCode::Retryable),
        3 => Ok(GeneratedRetryDispositionCode::RetryAfter),
        4 => Ok(GeneratedRetryDispositionCode::Backpressure),
        0 => protocol_error("generated error retry_disposition must be specified"),
        _ => protocol_error("unknown generated error retry_disposition"),
    }
}

fn validate_backpressure_metadata<T>(backpressure: &T) -> AndromedaResult<()>
where
    T: GeneratedBackpressureMetadataView,
{
    if matches!(backpressure.capacity_percent(), Some(percent) if percent > 100) {
        return resource_error("backpressure capacity percent must be <= 100");
    }

    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GeneratedRetryDispositionCode {
    NotRetryable,
    Retryable,
    RetryAfter,
    Backpressure,
}
