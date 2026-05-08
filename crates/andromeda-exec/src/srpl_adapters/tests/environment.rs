use super::fixtures::integer_row;
use super::*;
use andromeda_srpl_execution_adapter::{SrplBindingEnvironment, SrplBoundValue};

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
    let rows = vec![integer_row("id", 1), integer_row("id", 2)];
    env.bind_read("users", rows.clone());

    assert_eq!(env.get_binding_internal("users"), Some(&rows));
    assert_eq!(env.get_binding_internal("missing"), None);
}

#[test]
fn binding_environment_trait_get_input() {
    let mut env = SrplTypedEnvironment::new();
    env.add_input("user_id", FieldValue::Integer(42));
    env.add_input("name", FieldValue::String("Alice".to_string()));

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
    let row1 = integer_row("id", 1).with_field("status", FieldValue::String("active".to_string()));
    let row2 =
        integer_row("id", 2).with_field("status", FieldValue::String("inactive".to_string()));
    env.bind_read("users", vec![row1, row2]);

    let binding: &dyn SrplBindingEnvironment = &env;

    assert_eq!(
        binding.get_field_from_binding("users", 0, "id"),
        Some(SrplBoundValue::Integer(1))
    );
    assert_eq!(
        binding.get_field_from_binding("users", 0, "status"),
        Some(SrplBoundValue::String("active".to_string()))
    );
    assert_eq!(
        binding.get_field_from_binding("users", 1, "id"),
        Some(SrplBoundValue::Integer(2))
    );
    assert_eq!(binding.get_field_from_binding("users", 2, "id"), None);
    assert_eq!(binding.get_field_from_binding("missing", 0, "id"), None);
    assert_eq!(
        binding.get_field_from_binding("users", 0, "missing_field"),
        None
    );
}

#[test]
fn binding_environment_trait_binding_row_count() {
    let mut env = SrplTypedEnvironment::new();
    env.bind_read("empty", vec![]);
    env.bind_read("single", vec![integer_row("x", 1)]);
    env.bind_read(
        "multiple",
        vec![
            integer_row("x", 1),
            integer_row("x", 2),
            integer_row("x", 3),
        ],
    );

    let binding: &dyn SrplBindingEnvironment = &env;

    assert_eq!(binding.binding_row_count("empty"), Some(0));
    assert_eq!(binding.binding_row_count("single"), Some(1));
    assert_eq!(binding.binding_row_count("multiple"), Some(3));
    assert_eq!(binding.binding_row_count("missing"), None);
}
