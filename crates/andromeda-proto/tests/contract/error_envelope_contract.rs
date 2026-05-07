use andromeda_core::{AndromedaErrorKind, RequestId, SessionId};
use andromeda_proto::{
    BackpressureMetadata, ErrorEnvelope, ErrorFamily, RetryDisposition, TransactionEffect,
    generated, validate_generated_error_envelope,
};
use generated::protocol::v1::{ErrorEnvelope as ProtoErrorEnvelope, error_envelope};

use super::support::{PROTOCOL_SCHEMAS, declared_message_names, schema_contains};

fn valid_error_envelope() -> ErrorEnvelope {
    ErrorEnvelope {
        request_id: Some(RequestId::new(101)),
        session_id: Some(SessionId::new(202)),
        trace_id: Some("trace-proto-101".to_string()),
        family: ErrorFamily::Contract,
        code: "CONTRACT_HASH_MISMATCH".to_string(),
        message: "ContractHash mismatch".to_string(),
        transaction_effect: TransactionEffect::NoTransaction,
        retry_disposition: RetryDisposition::Backpressure,
        retry_after_ms: None,
        backpressure: Some(BackpressureMetadata {
            retry_after_ms: Some(50),
            capacity_percent: Some(70),
            shed_load: true,
        }),
    }
}

fn valid_generated_error_envelope() -> ProtoErrorEnvelope {
    ProtoErrorEnvelope {
        request_id: Some(101),
        session_id: Some(202),
        trace_id: Some("trace-proto-101".to_string()),
        family: error_envelope::ErrorFamily::Contract as i32,
        code: "CONTRACT_HASH_MISMATCH".to_string(),
        message: "ContractHash mismatch".to_string(),
        transaction_effect: error_envelope::TransactionEffect::NoTransaction as i32,
        retry_disposition: error_envelope::RetryDisposition::Backpressure as i32,
        retry_after_ms: None,
        backpressure: Some(error_envelope::BackpressureMetadata {
            retry_after_ms: Some(50),
            capacity_percent: Some(70),
            shed_load: true,
        }),
    }
}

#[test]
fn error_envelope_schema_declares_typed_error_contract() {
    let declared_messages = declared_message_names();

    for message in ["ErrorEnvelope", "BackpressureMetadata"] {
        assert!(
            declared_messages.contains(message),
            "protocol schema must include {message}"
        );
    }

    for field in [
        "optional BackpressureMetadata backpressure = 10;",
        "ERROR_FAMILY_CONTRACT = 4;",
        "TRANSACTION_EFFECT_ROLLBACK_REQUIRED = 2;",
        "RETRY_DISPOSITION_BACKPRESSURE = 4;",
        "reserved 11 to 31;",
    ] {
        assert!(
            schema_contains(PROTOCOL_SCHEMAS, field),
            "protocol schema missing ErrorEnvelope contract field: {field}"
        );
    }
}

#[test]
fn error_envelope_requires_retry_metadata_for_backpressure_and_retry_after() {
    assert!(valid_error_envelope().validate().is_ok());

    let missing_backpressure = ErrorEnvelope {
        backpressure: None,
        ..valid_error_envelope()
    };
    assert_eq!(
        missing_backpressure.validate().unwrap_err().kind(),
        AndromedaErrorKind::Resource
    );

    let retry_after_without_delay = ErrorEnvelope {
        retry_disposition: RetryDisposition::RetryAfter,
        retry_after_ms: None,
        backpressure: None,
        ..valid_error_envelope()
    };
    assert_eq!(
        retry_after_without_delay.validate().unwrap_err().kind(),
        AndromedaErrorKind::Resource
    );
}

#[test]
fn generated_error_envelope_projects_to_typed_runtime_validation() {
    let valid = valid_generated_error_envelope();
    validate_generated_error_envelope(&valid).unwrap();

    let empty_code = ProtoErrorEnvelope {
        code: String::new(),
        ..valid.clone()
    };
    assert_eq!(
        validate_generated_error_envelope(&empty_code)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Contract
    );

    let over_capacity = ProtoErrorEnvelope {
        backpressure: Some(error_envelope::BackpressureMetadata {
            capacity_percent: Some(101),
            ..valid.backpressure.unwrap()
        }),
        ..valid
    };
    assert_eq!(
        validate_generated_error_envelope(&over_capacity)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Resource
    );
}
