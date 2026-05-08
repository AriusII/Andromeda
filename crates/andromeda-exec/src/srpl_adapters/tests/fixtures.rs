use super::*;
use andromeda_srpl_execution_adapter::SrplExecutionFailure;
use andromeda_srpl_ir::SrplPredicateIr;

pub(super) const DEFAULT_BUFFERED_ROWS: usize = 1_000;

pub(super) fn new_adapter() -> SrplExecutionAdapter {
    SrplExecutionAdapter::new(DEFAULT_BUFFERED_ROWS)
}

pub(super) fn integer_row(field: &str, value: i64) -> StructuredObject {
    StructuredObject::new().with_field(field, FieldValue::Integer(value))
}

pub(super) fn string_row(field: &str, value: &str) -> StructuredObject {
    StructuredObject::new().with_field(field, FieldValue::String(value.to_string()))
}

pub(super) fn input_equals_field(input: &str, binding: &str, field: &str) -> SrplPredicateIr {
    SrplPredicateIr::InputEqualsField {
        input: input.to_string(),
        binding: binding.to_string(),
        field: field.to_string(),
    }
}

pub(super) fn field_gte_input(binding: &str, field: &str, input: &str) -> SrplPredicateIr {
    SrplPredicateIr::FieldGreaterThanOrEqualInput {
        binding: binding.to_string(),
        field: field.to_string(),
        input: input.to_string(),
    }
}

pub(super) fn assert_semantic_violation_contains<T>(
    result: Result<T, SrplExecutionFailure>,
    expected: &str,
) {
    match result {
        Err(SrplExecutionFailure::SemanticViolation(message)) => assert!(
            message.contains(expected),
            "expected semantic violation containing '{expected}', got '{message}'"
        ),
        Err(error) => panic!("expected semantic violation, got {error:?}"),
        Ok(_) => panic!("expected semantic violation, got Ok"),
    }
}
