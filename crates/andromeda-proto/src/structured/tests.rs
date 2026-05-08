use super::*;
use andromeda_error::AndromedaErrorKind;
use andromeda_types::{ColumnDescriptor, ContractHash, ScalarType, TypeDescriptor};

fn fields() -> Vec<ColumnDescriptor> {
    vec![
        ColumnDescriptor {
            name: "ProductId".to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal: 0,
        },
        ColumnDescriptor {
            name: "Quantity".to_string(),
            data_type: TypeDescriptor::required(ScalarType::I32),
            ordinal: 1,
        },
    ]
}

fn header(name: &str) -> StructuredObjectHeader {
    let layout = StructuredObjectLayout::RowMajor;
    let fields = fields();
    let descriptor_hash = StructuredObjectHeader::compute_descriptor_hash(&fields, layout);
    StructuredObjectHeader {
        name: name.to_string(),
        contract_hash: ContractHash::test_vector(7),
        descriptor_hash,
        column_count: fields.len() as u32,
        fields,
        layout,
        row_count_policy: RowCountPolicy::ExactRequired,
        row_count_exact: Some(1),
        payload_length: 64,
        payload_checksum: Some(0xA5A5),
        max_payload_length: Some(256),
    }
}

#[test]
fn descriptor_hash_is_deterministic_and_independent_of_name() {
    let a = header("Reservation");
    let b = header("OtherName");
    assert_eq!(a.descriptor_hash, b.descriptor_hash);

    assert_eq!(
        StructuredObjectHeader::compute_descriptor_hash(
            &fields(),
            StructuredObjectLayout::RowMajor
        ),
        StructuredObjectHeader::compute_descriptor_hash(
            &fields(),
            StructuredObjectLayout::RowMajor
        )
    );
}

#[test]
fn descriptor_hash_changes_when_layout_changes() {
    let row_major = StructuredObjectHeader::compute_descriptor_hash(
        &fields(),
        StructuredObjectLayout::RowMajor,
    );
    let column_major = StructuredObjectHeader::compute_descriptor_hash(
        &fields(),
        StructuredObjectLayout::ColumnMajor,
    );
    assert_ne!(row_major, column_major);
}

#[test]
fn descriptor_hash_changes_when_field_set_changes() {
    let mut alt = fields();
    alt[1].name = "Renamed".to_string();
    assert_ne!(
        StructuredObjectHeader::compute_descriptor_hash(
            &fields(),
            StructuredObjectLayout::RowMajor
        ),
        StructuredObjectHeader::compute_descriptor_hash(&alt, StructuredObjectLayout::RowMajor)
    );
}

#[test]
fn validate_rejects_descriptor_hash_drift() {
    let mut h = header("Reservation");
    h.descriptor_hash = ContractHash::test_vector(0xAA);
    let err = h.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("descriptor hash"));
}

#[test]
fn validate_rejects_column_count_mismatch() {
    let mut h = header("Reservation");
    h.column_count = (h.fields.len() as u32) + 1;
    let err = h.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("column_count"));
}

#[test]
fn validate_rejects_exact_required_without_exact_value() {
    let mut h = header("Reservation");
    h.row_count_policy = RowCountPolicy::ExactRequired;
    h.row_count_exact = None;
    let err = h.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("ExactRequired"));
}

#[test]
fn validate_accepts_unknown_allowed_without_exact_value() {
    let mut h = header("Reservation");
    h.row_count_policy = RowCountPolicy::UnknownAllowed;
    h.row_count_exact = None;
    assert!(h.validate().is_ok());
}

#[test]
fn validate_rejects_payload_when_zero_rows_declared() {
    let mut h = header("Reservation");
    h.row_count_exact = Some(0);
    h.payload_length = 1;
    let err = h.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(err.message().contains("zero rows"));
}

#[test]
fn validate_rejects_empty_payload_when_positive_rows_declared() {
    let mut h = header("Reservation");
    h.row_count_exact = Some(3);
    h.payload_length = 0;
    let err = h.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(err.message().contains("empty payload"));
}

#[test]
fn validate_rejects_payload_exceeding_max() {
    let mut h = header("Reservation");
    h.payload_length = h.max_payload_length.unwrap() + 1;
    let err = h.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(err.message().contains("payload length"));
}

#[test]
fn validate_accepts_well_formed_header() {
    assert!(header("Reservation").validate().is_ok());
}
