use super::*;
use andromeda_error::AndromedaErrorKind;
use andromeda_types::{
    AbsencePolicy, ColumnDescriptor, DecimalType, ScalarType, TextEncoding, TextType,
    TypeDescriptor,
};

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

fn refresh_descriptor_hash(header: &mut StructuredObjectHeader) {
    header.column_count = header.fields.len() as u32;
    header.descriptor_hash =
        StructuredObjectHeader::compute_descriptor_hash(&header.fields, header.layout);
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
fn descriptor_hash_matches_golden_row_major_two_column_fixture() {
    let descriptor_hash = StructuredObjectHeader::compute_descriptor_hash(
        &fields(),
        StructuredObjectLayout::RowMajor,
    );

    assert_eq!(
        descriptor_hash.to_string(),
        "e8548724193011fd2e357d0b7ad96a0c5740985647c3d5897ac798780c7f8755"
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
fn descriptor_hash_changes_when_absence_policy_changes() {
    let mut optional = fields();
    optional[1].data_type = TypeDescriptor::optional(ScalarType::I32);

    assert_ne!(
        StructuredObjectHeader::compute_descriptor_hash(
            &fields(),
            StructuredObjectLayout::RowMajor
        ),
        StructuredObjectHeader::compute_descriptor_hash(
            &optional,
            StructuredObjectLayout::RowMajor
        )
    );

    let mut h = header("Reservation");
    h.fields[1].data_type = TypeDescriptor::optional(ScalarType::I32);
    refresh_descriptor_hash(&mut h);

    assert_eq!(
        h.fields[1].data_type.absence,
        AbsencePolicy::ExplicitOptional
    );
    assert!(h.validate().is_ok());
}

#[test]
fn descriptor_hash_changes_when_decimal_bounds_change() {
    let mut amount = fields();
    amount.push(ColumnDescriptor {
        name: "Amount".to_string(),
        data_type: TypeDescriptor::required(ScalarType::Decimal(DecimalType::Custom {
            precision: 9,
            scale: 2,
        })),
        ordinal: 2,
    });

    let mut wider = amount.clone();
    wider[2].data_type = TypeDescriptor::required(ScalarType::Decimal(DecimalType::Custom {
        precision: 10,
        scale: 2,
    }));

    assert_ne!(
        StructuredObjectHeader::compute_descriptor_hash(&amount, StructuredObjectLayout::RowMajor),
        StructuredObjectHeader::compute_descriptor_hash(&wider, StructuredObjectLayout::RowMajor)
    );
}

#[test]
fn descriptor_hash_changes_when_text_bounds_change() {
    let mut text = fields();
    text.push(ColumnDescriptor {
        name: "Comment".to_string(),
        data_type: TypeDescriptor::required(ScalarType::Text(TextType {
            encoding: TextEncoding::Utf8,
            max_length: Some(32),
            collation: Some("unicode:case-sensitive".to_string()),
        })),
        ordinal: 2,
    });

    let mut longer = text.clone();
    longer[2].data_type = TypeDescriptor::required(ScalarType::Text(TextType {
        encoding: TextEncoding::Utf8,
        max_length: Some(64),
        collation: Some("unicode:case-sensitive".to_string()),
    }));

    assert_ne!(
        StructuredObjectHeader::compute_descriptor_hash(&text, StructuredObjectLayout::RowMajor),
        StructuredObjectHeader::compute_descriptor_hash(&longer, StructuredObjectLayout::RowMajor)
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
fn validate_rejects_stale_descriptor_hash_after_shape_change() {
    let mut h = header("Reservation");
    h.fields[1].data_type = TypeDescriptor::optional(ScalarType::I32);

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
fn validate_rejects_invalid_decimal_field_descriptor() {
    let mut h = header("Reservation");
    h.fields[1].data_type = TypeDescriptor::required(ScalarType::Decimal(DecimalType::Custom {
        precision: 2,
        scale: 3,
    }));
    refresh_descriptor_hash(&mut h);

    let err = h.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("decimal"));
}

#[test]
fn validate_rejects_invalid_text_bounds_or_collation() {
    let mut zero_length = header("Reservation");
    zero_length.fields[1].data_type = TypeDescriptor::required(ScalarType::Text(TextType {
        encoding: TextEncoding::Utf8,
        max_length: Some(0),
        collation: None,
    }));
    refresh_descriptor_hash(&mut zero_length);

    let err = zero_length.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("max length"));

    let mut empty_collation = header("Reservation");
    empty_collation.fields[1].data_type = TypeDescriptor::required(ScalarType::Text(TextType {
        encoding: TextEncoding::Utf8,
        max_length: Some(64),
        collation: Some(" ".to_string()),
    }));
    refresh_descriptor_hash(&mut empty_collation);

    let err = empty_collation.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("collation"));
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
fn validate_accepts_zero_rows_with_empty_payload() {
    let mut h = header("Reservation");
    h.row_count_exact = Some(0);
    h.payload_length = 0;
    h.payload_checksum = None;

    assert!(h.validate().is_ok());
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
