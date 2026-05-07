use std::sync::Arc;

use super::*;

fn create_test_schema() -> Arc<RowSchema> {
    Arc::new(
        RowSchema::new(vec![
            ColumnDef {
                name: "id".to_string(),
                ordinal: 0,
                scalar_type: ScalarType::Int64,
                nullable: false,
            },
            ColumnDef {
                name: "name".to_string(),
                ordinal: 1,
                scalar_type: ScalarType::UInt32,
                nullable: true,
            },
            ColumnDef {
                name: "active".to_string(),
                ordinal: 2,
                scalar_type: ScalarType::Bool,
                nullable: true,
            },
        ])
        .expect("schema creation failed"),
    )
}

#[test]
fn test_null_bitmap_encoding() {
    let values = vec![Datum::Int64(42), Datum::Null, Datum::Bool(true)];
    let bitmap = RowEncoder::encode_null_bitmap(&values).expect("encode failed");

    assert_eq!(bitmap[0], 0x02);
}

#[test]
fn test_scalar_type_byte_lengths() {
    assert_eq!(ScalarType::Int8.fixed_byte_length(), 1);
    assert_eq!(ScalarType::Int16.fixed_byte_length(), 2);
    assert_eq!(ScalarType::Int32.fixed_byte_length(), 4);
    assert_eq!(ScalarType::Int64.fixed_byte_length(), 8);
    assert_eq!(ScalarType::Float32.fixed_byte_length(), 4);
    assert_eq!(ScalarType::Float64.fixed_byte_length(), 8);
    assert_eq!(ScalarType::Bool.fixed_byte_length(), 1);
}

#[test]
fn test_datum_byte_lengths() {
    assert_eq!(Datum::Int64(42).byte_length().unwrap(), 8);
    assert_eq!(Datum::Bool(true).byte_length().unwrap(), 1);
    assert_eq!(Datum::Bytes(vec![1, 2, 3]).byte_length().unwrap(), 3);
}

#[test]
fn test_datum_roundtrip_int64() {
    let original = Datum::Int64(12345);
    let encoded = original.encode().expect("encode failed");
    let decoded = Datum::decode_scalar(ScalarType::Int64, &encoded).expect("decode failed");
    assert_eq!(original, decoded);
}

#[test]
fn test_datum_roundtrip_bool() {
    let original = Datum::Bool(true);
    let encoded = original.encode().expect("encode failed");
    let decoded = Datum::decode_scalar(ScalarType::Bool, &encoded).expect("decode failed");
    assert_eq!(original, decoded);
}

#[test]
fn test_row_encoder_roundtrip() {
    let schema = create_test_schema();
    let encoder = RowEncoder::new(schema.clone());

    let values = vec![Datum::Int64(42), Datum::Null, Datum::Bool(true)];

    let encoded = encoder.encode(&values).expect("encode failed");
    let decoded = encoder.decode(&encoded).expect("decode failed");

    assert_eq!(decoded[0], Datum::Int64(42));
    assert_eq!(decoded[1], Datum::Null);
    assert_eq!(decoded[2], Datum::Bool(true));
}

#[test]
fn test_row_encoder_mismatch() {
    let schema = create_test_schema();
    let encoder = RowEncoder::new(schema.clone());

    let values = vec![Datum::Int64(42)];
    let result = encoder.encode(&values);

    assert!(result.is_err());
}

#[test]
fn test_row_encoder_rejects_null_for_non_nullable_column() {
    let schema = create_test_schema();
    let encoder = RowEncoder::new(schema);

    let values = vec![Datum::Null, Datum::Null, Datum::Bool(true)];
    let error = encoder
        .encode(&values)
        .expect_err("non-nullable column must reject Null");

    assert!(error.message().contains("not nullable"));
}

#[test]
fn test_row_encoder_rejects_type_mismatch() {
    let schema = create_test_schema();
    let encoder = RowEncoder::new(schema);

    let values = vec![Datum::UInt64(42), Datum::Null, Datum::Bool(true)];
    let error = encoder
        .encode(&values)
        .expect_err("schema type mismatch must be rejected");

    assert!(error.message().contains("expected Int64"));
}

#[test]
fn test_row_encoder_decode_rejects_non_nullable_null_bitmap() {
    let schema = create_test_schema();
    let encoder = RowEncoder::new(schema);
    let values = vec![Datum::Int64(42), Datum::Null, Datum::Bool(true)];
    let mut encoded = encoder.encode(&values).expect("encode");
    encoded[0] |= 0x01;

    let error = encoder
        .decode(&encoded)
        .expect_err("non-nullable null bitmap must be rejected");

    assert!(error.message().contains("not nullable"));
}
