use super::*;
use andromeda_srpl_execution_adapter::SrplBoundValue;

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
