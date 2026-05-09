use andromeda_error::AndromedaErrorKind;
use andromeda_procedure_contract::{
    ResultCardinality, ResultStreamDescriptor, RowCountRequirement,
};

use super::support::{
    CONTRACT_SCHEMAS, PROTOCOL_SCHEMAS, declared_message_names,
    governance_result_stream_descriptor, schema_contains,
};

#[test]
fn result_stream_schema_declares_metadata_batch_completion_sequence_contracts() {
    let declared_messages = declared_message_names();

    for message in [
        "RpcMetadata",
        "RpcBatch",
        "ResultCompletionPolicy",
        "InvocationResponse",
    ] {
        assert!(
            declared_messages.contains(message),
            "protocol schema must include {message}"
        );
    }
    assert!(
        declared_messages.contains("ResultStreamDescriptor"),
        "contract schema must include ResultStreamDescriptor"
    );

    for field in [
        "CompletionShape completion_shape = 1;",
        "bytes structured_payload = 4;",
        "bool terminal_batch = 6;",
    ] {
        assert!(
            schema_contains(PROTOCOL_SCHEMAS, field),
            "protocol schema missing ResultStream sequence field: {field}"
        );
    }

    assert!(
        schema_contains(CONTRACT_SCHEMAS, "optional uint64 row_count_max = 6;"),
        "contract schema missing ResultStream row_count_max bound"
    );
}

#[test]
fn result_stream_descriptor_aligns_cardinality_and_exact_row_count_requirement() {
    let descriptor = governance_result_stream_descriptor();

    assert!(descriptor.validate().is_ok());

    let missing_exact = ResultStreamDescriptor {
        row_count_exact: None,
        ..descriptor.clone()
    };
    assert_eq!(
        missing_exact.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let wrong_cardinality = ResultStreamDescriptor {
        row_count_exact: Some(2),
        row_count_max: Some(2),
        ..descriptor
    };
    assert_eq!(
        wrong_cardinality.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );
}

#[test]
fn result_stream_exact_required_descriptor_is_contract_versioned() {
    let descriptor = governance_result_stream_descriptor();

    assert_eq!(descriptor.cardinality, ResultCardinality::ExactlyOne);
    assert_eq!(
        descriptor.row_count_requirement,
        RowCountRequirement::ExactRequired
    );
    assert_eq!(descriptor.row_count_exact, Some(1));
    assert_eq!(descriptor.row_count_max, Some(1));
}
