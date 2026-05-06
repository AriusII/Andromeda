use super::*;
use andromeda_srpl::{
    execution_adapter::{SrplBindingEnvironment, SrplBoundValue, SrplExecutionFailure},
    procedure_model::SrplPredicateIr,
};

#[test]
fn structured_object_field_access() {
    let obj = StructuredObject::new()
        .with_field("id", FieldValue::Integer(42))
        .with_field("name", FieldValue::String("test".to_string()));

    assert_eq!(obj.get_field("id"), Some(&FieldValue::Integer(42)));
    assert_eq!(
        obj.get_field("name"),
        Some(&FieldValue::String("test".to_string()))
    );
    assert_eq!(obj.get_field("missing"), None);
}

#[test]
fn field_value_conversions() {
    assert_eq!(FieldValue::Integer(42).as_i64(), Some(42));
    assert_eq!(
        FieldValue::String("hello".to_string()).as_string(),
        Some("hello")
    );
    assert_eq!(FieldValue::Bool(true).as_bool(), Some(true));

    assert_eq!(FieldValue::String("x".to_string()).as_i64(), None);
}

#[test]
fn srpl_typed_environment_inputs() {
    let mut env = SrplTypedEnvironment::new();
    env.add_input("param1", FieldValue::Integer(100));
    env.add_input("param2", FieldValue::String("data".to_string()));

    assert_eq!(
        env.get_input_internal("param1"),
        Some(&FieldValue::Integer(100))
    );
    assert_eq!(
        env.get_input_internal("param2"),
        Some(&FieldValue::String("data".to_string()))
    );
    assert_eq!(env.get_input_internal("missing"), None);
}

#[test]
fn srpl_typed_environment_bindings() {
    let mut env = SrplTypedEnvironment::new();
    let rows = vec![
        StructuredObject::new().with_field("id", FieldValue::Integer(1)),
        StructuredObject::new().with_field("id", FieldValue::Integer(2)),
    ];
    env.bind_read("users", rows.clone());

    assert_eq!(env.get_binding_internal("users"), Some(&rows));
    assert_eq!(env.get_binding_internal("missing"), None);
}

#[test]
fn srpl_transaction_context_records_updates() {
    let mut tx = SrplTransactionContext::new();
    let obj =
        StructuredObject::new().with_field("status", FieldValue::String("active".to_string()));

    assert!(tx.record_update("users", 0, obj.clone()).is_ok());
    assert_eq!(tx.pending_updates().len(), 1);
}

#[test]
fn srpl_transaction_context_abort_prevents_updates() {
    let mut tx = SrplTransactionContext::new();
    tx.abort();

    let obj = StructuredObject::new();
    let result = tx.record_update("users", 0, obj);
    assert!(result.is_err());
}

#[test]
fn srpl_backpressure_respects_limits() {
    let mut bp = SrplStreamBackpressure::new(100);

    assert!(bp.buffer_rows(50).is_ok());
    assert!(bp.buffer_rows(50).is_ok());
    assert!(bp.buffer_rows(1).is_err());
}

#[test]
fn srpl_backpressure_release() {
    let mut bp = SrplStreamBackpressure::new(100);
    bp.buffer_rows(80).ok();
    bp.release_rows(30);
    assert!(bp.buffer_rows(50).is_ok());
}

#[test]
fn srpl_execution_adapter_creation() {
    let adapter = SrplExecutionAdapter::new(1000);
    assert!(!adapter.is_transaction_aborted());
    assert_eq!(adapter.failures().len(), 0);
}

#[test]
fn srpl_execution_adapter_abort_transaction() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    assert!(!adapter.is_transaction_aborted());
    adapter.abort_transaction();
    assert!(adapter.is_transaction_aborted());
}

#[test]
fn srpl_predicate_evaluation_input_equals_field() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    adapter.add_input("id", FieldValue::Integer(42));

    let row = StructuredObject::new().with_field("user_id", FieldValue::Integer(42));
    adapter.environment_mut().bind_read("users", vec![row]);

    let predicate = SrplPredicateIr::InputEqualsField {
        input: "id".to_string(),
        binding: "users".to_string(),
        field: "user_id".to_string(),
    };

    let result = adapter.evaluate_predicate(&predicate);
    assert_eq!(result, Ok(true));
}

#[test]
fn srpl_predicate_evaluation_field_gte_input() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    adapter.add_input("threshold", FieldValue::Integer(10));

    let row = StructuredObject::new().with_field("quantity", FieldValue::Integer(15));
    adapter.environment_mut().bind_read("items", vec![row]);

    let predicate = SrplPredicateIr::FieldGreaterThanOrEqualInput {
        binding: "items".to_string(),
        field: "quantity".to_string(),
        input: "threshold".to_string(),
    };

    let result = adapter.evaluate_predicate(&predicate);
    assert_eq!(result, Ok(true));
}

#[test]
fn srpl_predicate_evaluation_missing_input() {
    let adapter = SrplExecutionAdapter::new(1000);

    let predicate = SrplPredicateIr::InputEqualsField {
        input: "missing_id".to_string(),
        binding: "users".to_string(),
        field: "user_id".to_string(),
    };

    let result = adapter.evaluate_predicate(&predicate);
    assert!(matches!(
        result,
        Err(SrplExecutionFailure::SemanticViolation(_))
    ));
}

#[test]
fn srpl_predicate_evaluation_missing_binding() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    adapter.add_input("id", FieldValue::Integer(42));

    let predicate = SrplPredicateIr::InputEqualsField {
        input: "id".to_string(),
        binding: "missing_binding".to_string(),
        field: "user_id".to_string(),
    };

    let result = adapter.evaluate_predicate(&predicate);
    assert!(matches!(
        result,
        Err(SrplExecutionFailure::SemanticViolation(_))
    ));
}

#[test]
fn binding_environment_trait_get_input() {
    let mut env = SrplTypedEnvironment::new();
    env.add_input("user_id", FieldValue::Integer(42));
    env.add_input("name", FieldValue::String("Alice".to_string()));

    // Using the trait interface
    let binding: &dyn SrplBindingEnvironment = &env;
    assert_eq!(
        binding.get_input("user_id"),
        Some(SrplBoundValue::Integer(42))
    );
    assert_eq!(
        binding.get_input("name"),
        Some(SrplBoundValue::String("Alice".to_string()))
    );
    assert_eq!(binding.get_input("missing"), None);
}

#[test]
fn binding_environment_trait_get_field_from_binding() {
    let mut env = SrplTypedEnvironment::new();
    let row1 = StructuredObject::new()
        .with_field("id", FieldValue::Integer(1))
        .with_field("status", FieldValue::String("active".to_string()));
    let row2 = StructuredObject::new()
        .with_field("id", FieldValue::Integer(2))
        .with_field("status", FieldValue::String("inactive".to_string()));
    env.bind_read("users", vec![row1, row2]);

    let binding: &dyn SrplBindingEnvironment = &env;

    // Access first row
    assert_eq!(
        binding.get_field_from_binding("users", 0, "id"),
        Some(SrplBoundValue::Integer(1))
    );
    assert_eq!(
        binding.get_field_from_binding("users", 0, "status"),
        Some(SrplBoundValue::String("active".to_string()))
    );

    // Access second row
    assert_eq!(
        binding.get_field_from_binding("users", 1, "id"),
        Some(SrplBoundValue::Integer(2))
    );

    // Missing row index
    assert_eq!(binding.get_field_from_binding("users", 2, "id"), None);

    // Missing binding
    assert_eq!(binding.get_field_from_binding("missing", 0, "id"), None);

    // Missing field
    assert_eq!(
        binding.get_field_from_binding("users", 0, "missing_field"),
        None
    );
}

#[test]
fn binding_environment_trait_binding_row_count() {
    let mut env = SrplTypedEnvironment::new();
    env.bind_read("empty", vec![]);
    env.bind_read(
        "single",
        vec![StructuredObject::new().with_field("x", FieldValue::Integer(1))],
    );
    env.bind_read(
        "multiple",
        vec![
            StructuredObject::new().with_field("x", FieldValue::Integer(1)),
            StructuredObject::new().with_field("x", FieldValue::Integer(2)),
            StructuredObject::new().with_field("x", FieldValue::Integer(3)),
        ],
    );

    let binding: &dyn SrplBindingEnvironment = &env;

    assert_eq!(binding.binding_row_count("empty"), Some(0));
    assert_eq!(binding.binding_row_count("single"), Some(1));
    assert_eq!(binding.binding_row_count("multiple"), Some(3));
    assert_eq!(binding.binding_row_count("missing"), None);
}

#[test]
fn predicate_evaluation_with_environment_input_equals_field() {
    let adapter = SrplExecutionAdapter::new(1000);
    let mut env = SrplTypedEnvironment::new();

    env.add_input("order_id", FieldValue::Integer(42));
    let row = StructuredObject::new().with_field("id", FieldValue::Integer(42));
    env.bind_read("orders", vec![row]);

    let predicate = SrplPredicateIr::InputEqualsField {
        input: "order_id".to_string(),
        binding: "orders".to_string(),
        field: "id".to_string(),
    };

    let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
    assert_eq!(result, Ok(true));
}

#[test]
fn predicate_evaluation_with_environment_input_not_equals_field() {
    let adapter = SrplExecutionAdapter::new(1000);
    let mut env = SrplTypedEnvironment::new();

    env.add_input("order_id", FieldValue::Integer(42));
    let row = StructuredObject::new().with_field("id", FieldValue::Integer(99));
    env.bind_read("orders", vec![row]);

    let predicate = SrplPredicateIr::InputEqualsField {
        input: "order_id".to_string(),
        binding: "orders".to_string(),
        field: "id".to_string(),
    };

    let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
    assert_eq!(result, Ok(false));
}

#[test]
fn predicate_evaluation_with_environment_field_gte_input() {
    let adapter = SrplExecutionAdapter::new(1000);
    let mut env = SrplTypedEnvironment::new();

    env.add_input("min_stock", FieldValue::Integer(10));
    let row = StructuredObject::new().with_field("available", FieldValue::Integer(15));
    env.bind_read("inventory", vec![row]);

    let predicate = SrplPredicateIr::FieldGreaterThanOrEqualInput {
        binding: "inventory".to_string(),
        field: "available".to_string(),
        input: "min_stock".to_string(),
    };

    let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
    assert_eq!(result, Ok(true));
}

#[test]
fn predicate_evaluation_with_environment_field_not_gte_input() {
    let adapter = SrplExecutionAdapter::new(1000);
    let mut env = SrplTypedEnvironment::new();

    env.add_input("min_stock", FieldValue::Integer(20));
    let row = StructuredObject::new().with_field("available", FieldValue::Integer(15));
    env.bind_read("inventory", vec![row]);

    let predicate = SrplPredicateIr::FieldGreaterThanOrEqualInput {
        binding: "inventory".to_string(),
        field: "available".to_string(),
        input: "min_stock".to_string(),
    };

    let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
    assert_eq!(result, Ok(false));
}

#[test]
fn predicate_evaluation_empty_binding_returns_false() {
    let adapter = SrplExecutionAdapter::new(1000);
    let mut env = SrplTypedEnvironment::new();

    env.add_input("id", FieldValue::Integer(42));
    env.bind_read("users", vec![]); // Empty binding

    let predicate = SrplPredicateIr::InputEqualsField {
        input: "id".to_string(),
        binding: "users".to_string(),
        field: "user_id".to_string(),
    };

    let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
    assert_eq!(result, Ok(false));
}

#[test]
fn predicate_evaluation_with_environment_missing_input_error() {
    let adapter = SrplExecutionAdapter::new(1000);
    let mut env = SrplTypedEnvironment::new();

    let row = StructuredObject::new().with_field("id", FieldValue::Integer(42));
    env.bind_read("users", vec![row]);

    let predicate = SrplPredicateIr::InputEqualsField {
        input: "missing_input".to_string(),
        binding: "users".to_string(),
        field: "id".to_string(),
    };

    let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
    assert!(
        matches!(result, Err(SrplExecutionFailure::SemanticViolation(msg)) if msg.contains("missing_input"))
    );
}

#[test]
fn predicate_evaluation_with_environment_missing_binding_error() {
    let adapter = SrplExecutionAdapter::new(1000);
    let mut env = SrplTypedEnvironment::new();

    env.add_input("id", FieldValue::Integer(42));

    let predicate = SrplPredicateIr::InputEqualsField {
        input: "id".to_string(),
        binding: "missing_binding".to_string(),
        field: "id".to_string(),
    };

    let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
    assert!(
        matches!(result, Err(SrplExecutionFailure::SemanticViolation(msg)) if msg.contains("missing_binding"))
    );
}

#[test]
fn predicate_evaluation_with_environment_missing_field_error() {
    let adapter = SrplExecutionAdapter::new(1000);
    let mut env = SrplTypedEnvironment::new();

    env.add_input("id", FieldValue::Integer(42));
    let row = StructuredObject::new().with_field("other_field", FieldValue::Integer(42));
    env.bind_read("users", vec![row]);

    let predicate = SrplPredicateIr::InputEqualsField {
        input: "id".to_string(),
        binding: "users".to_string(),
        field: "missing_field".to_string(),
    };

    let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
    assert!(
        matches!(result, Err(SrplExecutionFailure::SemanticViolation(msg)) if msg.contains("missing_field"))
    );
}

#[test]
fn predicate_evaluation_with_environment_type_mismatch_error() {
    let adapter = SrplExecutionAdapter::new(1000);
    let mut env = SrplTypedEnvironment::new();

    env.add_input("text", FieldValue::String("hello".to_string()));
    let row = StructuredObject::new().with_field("value", FieldValue::String("world".to_string()));
    env.bind_read("data", vec![row]);

    let predicate = SrplPredicateIr::FieldGreaterThanOrEqualInput {
        binding: "data".to_string(),
        field: "value".to_string(),
        input: "text".to_string(),
    };

    let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
    assert!(
        matches!(result, Err(SrplExecutionFailure::SemanticViolation(msg)) if msg.contains("integer"))
    );
}

#[test]
fn predicate_validation_with_environment() {
    let adapter = SrplExecutionAdapter::new(1000);
    let mut env = SrplTypedEnvironment::new();

    env.add_input("id", FieldValue::Integer(42));

    let predicate = SrplPredicateIr::InputEqualsField {
        input: "id".to_string(),
        binding: "users".to_string(),
        field: "id".to_string(),
    };

    // Validation should pass (binding may not exist yet, will be populated by read)
    let result = adapter.evaluate_predicate_for_binding(&predicate, &env);
    assert!(result.is_ok());
}

#[test]
fn predicate_validation_with_environment_missing_input() {
    let adapter = SrplExecutionAdapter::new(1000);
    let env = SrplTypedEnvironment::new();

    let predicate = SrplPredicateIr::InputEqualsField {
        input: "missing".to_string(),
        binding: "users".to_string(),
        field: "id".to_string(),
    };

    // Validation should fail (input does not exist)
    let result = adapter.evaluate_predicate_for_binding(&predicate, &env);
    assert!(
        matches!(result, Err(SrplExecutionFailure::SemanticViolation(msg)) if msg.contains("missing"))
    );
}

#[test]
fn multiple_inputs_and_bindings_in_environment() {
    let adapter = SrplExecutionAdapter::new(1000);
    let mut env = SrplTypedEnvironment::new();

    // Set up multiple inputs
    env.add_input("user_id", FieldValue::Integer(1));
    env.add_input("order_id", FieldValue::Integer(100));
    env.add_input("amount_threshold", FieldValue::Integer(50));

    // Set up multiple bindings
    env.bind_read(
        "users",
        vec![
            StructuredObject::new()
                .with_field("id", FieldValue::Integer(1))
                .with_field("name", FieldValue::String("Alice".to_string())),
        ],
    );
    env.bind_read(
        "orders",
        vec![
            StructuredObject::new()
                .with_field("id", FieldValue::Integer(100))
                .with_field("amount", FieldValue::Integer(75)),
        ],
    );

    // Check that all bindings can be resolved
    let pred1 = SrplPredicateIr::InputEqualsField {
        input: "user_id".to_string(),
        binding: "users".to_string(),
        field: "id".to_string(),
    };

    let pred2 = SrplPredicateIr::InputEqualsField {
        input: "order_id".to_string(),
        binding: "orders".to_string(),
        field: "id".to_string(),
    };

    let pred3 = SrplPredicateIr::FieldGreaterThanOrEqualInput {
        binding: "orders".to_string(),
        field: "amount".to_string(),
        input: "amount_threshold".to_string(),
    };

    assert_eq!(
        adapter.evaluate_predicate_with_environment(&pred1, &env),
        Ok(true)
    );
    assert_eq!(
        adapter.evaluate_predicate_with_environment(&pred2, &env),
        Ok(true)
    );
    assert_eq!(
        adapter.evaluate_predicate_with_environment(&pred3, &env),
        Ok(true)
    );
}

#[test]
fn field_value_conversion_to_srpl_bound_value() {
    assert_eq!(
        FieldValue::Integer(42).to_srpl_bound_value(),
        SrplBoundValue::Integer(42)
    );
    assert_eq!(
        FieldValue::String("test".to_string()).to_srpl_bound_value(),
        SrplBoundValue::String("test".to_string())
    );
    assert_eq!(
        FieldValue::Bool(true).to_srpl_bound_value(),
        SrplBoundValue::Bool(true)
    );
    assert_eq!(FieldValue::Null.to_srpl_bound_value(), SrplBoundValue::Null);
}

#[test]
fn field_value_roundtrip_conversion() {
    let original = FieldValue::Integer(42);
    let bound_value = original.to_srpl_bound_value();
    let reconstructed = FieldValue::from_srpl_bound_value(bound_value);
    assert_eq!(original, reconstructed);
}
