use andromeda_core::{AndromedaErrorKind, AndromedaResult, RequestId, SessionId};

use crate::generated::protocol;
use crate::{
    BackpressureMetadata, ErrorEnvelope, ErrorFamily, RetryDisposition, TransactionEffect,
};

use super::super::protocol_error;

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
