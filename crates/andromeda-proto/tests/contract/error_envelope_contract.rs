use super::support::{PROTOCOL_SCHEMAS, declared_message_names, schema_contains};

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
