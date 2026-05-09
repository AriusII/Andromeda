use super::fixtures::{
    assert_semantic_violation_contains, field_gte_input, input_equals_field, integer_row,
    new_adapter, string_row,
};
use super::*;

#[test]
fn srpl_predicate_evaluation_input_equals_field() {
    let mut adapter = new_adapter();
    adapter.add_input("id", FieldValue::Integer(42));
    adapter
        .environment_mut()
        .bind_read("users", vec![integer_row("user_id", 42)]);

    let result = adapter.evaluate_predicate(&input_equals_field("id", "users", "user_id"));
    assert_eq!(result, Ok(true));
}

#[test]
fn srpl_predicate_evaluation_field_gte_input() {
    let mut adapter = new_adapter();
    adapter.add_input("threshold", FieldValue::Integer(10));
    adapter
        .environment_mut()
        .bind_read("items", vec![integer_row("quantity", 15)]);

    let result = adapter.evaluate_predicate(&field_gte_input("items", "quantity", "threshold"));
    assert_eq!(result, Ok(true));
}

#[test]
fn srpl_predicate_evaluation_missing_input() {
    let adapter = new_adapter();

    let result = adapter.evaluate_predicate(&input_equals_field("missing_id", "users", "user_id"));
    assert_semantic_violation_contains(result, "missing_id");
}

#[test]
fn srpl_predicate_evaluation_missing_binding() {
    let mut adapter = new_adapter();
    adapter.add_input("id", FieldValue::Integer(42));

    let result =
        adapter.evaluate_predicate(&input_equals_field("id", "missing_binding", "user_id"));
    assert_semantic_violation_contains(result, "missing_binding");
}

#[test]
fn predicate_evaluation_with_environment_input_equals_field() {
    let adapter = new_adapter();
    let mut env = SrplTypedEnvironment::new();
    env.add_input("order_id", FieldValue::Integer(42));
    env.bind_read("orders", vec![integer_row("id", 42)]);

    let result = adapter
        .evaluate_predicate_with_environment(&input_equals_field("order_id", "orders", "id"), &env);
    assert_eq!(result, Ok(true));
}

#[test]
fn predicate_evaluation_with_environment_input_not_equals_field() {
    let adapter = new_adapter();
    let mut env = SrplTypedEnvironment::new();
    env.add_input("order_id", FieldValue::Integer(42));
    env.bind_read("orders", vec![integer_row("id", 99)]);

    let result = adapter
        .evaluate_predicate_with_environment(&input_equals_field("order_id", "orders", "id"), &env);
    assert_eq!(result, Ok(false));
}

#[test]
fn predicate_evaluation_with_environment_field_gte_input() {
    let adapter = new_adapter();
    let mut env = SrplTypedEnvironment::new();
    env.add_input("min_stock", FieldValue::Integer(10));
    env.bind_read("inventory", vec![integer_row("available", 15)]);

    let result = adapter.evaluate_predicate_with_environment(
        &field_gte_input("inventory", "available", "min_stock"),
        &env,
    );
    assert_eq!(result, Ok(true));
}

#[test]
fn predicate_evaluation_with_environment_field_not_gte_input() {
    let adapter = new_adapter();
    let mut env = SrplTypedEnvironment::new();
    env.add_input("min_stock", FieldValue::Integer(20));
    env.bind_read("inventory", vec![integer_row("available", 15)]);

    let result = adapter.evaluate_predicate_with_environment(
        &field_gte_input("inventory", "available", "min_stock"),
        &env,
    );
    assert_eq!(result, Ok(false));
}

#[test]
fn predicate_evaluation_empty_binding_returns_false() {
    let adapter = new_adapter();
    let mut env = SrplTypedEnvironment::new();
    env.add_input("id", FieldValue::Integer(42));
    env.bind_read("users", vec![]);

    let result = adapter
        .evaluate_predicate_with_environment(&input_equals_field("id", "users", "user_id"), &env);
    assert_eq!(result, Ok(false));
}

#[test]
fn predicate_evaluation_with_environment_missing_input_error() {
    let adapter = new_adapter();
    let mut env = SrplTypedEnvironment::new();
    env.bind_read("users", vec![integer_row("id", 42)]);

    let result = adapter.evaluate_predicate_with_environment(
        &input_equals_field("missing_input", "users", "id"),
        &env,
    );
    assert_semantic_violation_contains(result, "missing_input");
}

#[test]
fn predicate_evaluation_with_environment_missing_binding_error() {
    let adapter = new_adapter();
    let mut env = SrplTypedEnvironment::new();
    env.add_input("id", FieldValue::Integer(42));

    let result = adapter.evaluate_predicate_with_environment(
        &input_equals_field("id", "missing_binding", "id"),
        &env,
    );
    assert_semantic_violation_contains(result, "missing_binding");
}

#[test]
fn predicate_evaluation_with_environment_missing_field_error() {
    let adapter = new_adapter();
    let mut env = SrplTypedEnvironment::new();
    env.add_input("id", FieldValue::Integer(42));
    env.bind_read("users", vec![integer_row("other_field", 42)]);

    let result = adapter.evaluate_predicate_with_environment(
        &input_equals_field("id", "users", "missing_field"),
        &env,
    );
    assert_semantic_violation_contains(result, "missing_field");
}

#[test]
fn predicate_evaluation_with_environment_type_mismatch_error() {
    let adapter = new_adapter();
    let mut env = SrplTypedEnvironment::new();
    env.add_input("text", FieldValue::String("hello".to_string()));
    env.bind_read("data", vec![string_row("value", "world")]);

    let result = adapter
        .evaluate_predicate_with_environment(&field_gte_input("data", "value", "text"), &env);
    assert_semantic_violation_contains(result, "integer");
}

#[test]
fn predicate_validation_with_environment() {
    let adapter = new_adapter();
    let mut env = SrplTypedEnvironment::new();
    env.add_input("id", FieldValue::Integer(42));

    let result =
        adapter.evaluate_predicate_for_binding(&input_equals_field("id", "users", "id"), &env);
    assert!(result.is_ok());
}

#[test]
fn predicate_validation_with_environment_missing_input() {
    let adapter = new_adapter();
    let env = SrplTypedEnvironment::new();

    let result =
        adapter.evaluate_predicate_for_binding(&input_equals_field("missing", "users", "id"), &env);
    assert_semantic_violation_contains(result, "missing");
}

#[test]
fn multiple_inputs_and_bindings_resolve_predicates() {
    let adapter = new_adapter();
    let mut env = SrplTypedEnvironment::new();
    env.add_input("user_id", FieldValue::Integer(1));
    env.add_input("order_id", FieldValue::Integer(100));
    env.add_input("amount_threshold", FieldValue::Integer(50));
    env.bind_read(
        "users",
        vec![integer_row("id", 1).with_field("name", FieldValue::String("Alice".to_string()))],
    );
    env.bind_read(
        "orders",
        vec![integer_row("id", 100).with_field("amount", FieldValue::Integer(75))],
    );

    assert_eq!(
        adapter.evaluate_predicate_with_environment(
            &input_equals_field("user_id", "users", "id"),
            &env
        ),
        Ok(true)
    );
    assert_eq!(
        adapter.evaluate_predicate_with_environment(
            &input_equals_field("order_id", "orders", "id"),
            &env
        ),
        Ok(true)
    );
    assert_eq!(
        adapter.evaluate_predicate_with_environment(
            &field_gte_input("orders", "amount", "amount_threshold"),
            &env
        ),
        Ok(true)
    );
}
