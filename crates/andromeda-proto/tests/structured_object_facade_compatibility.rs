#![forbid(unsafe_code)]

use andromeda_procedure_contract::{
    ResultCardinality, ResultStreamDescriptor, RowCountRequirement,
};
use andromeda_proto::generated::contract::v1::result_stream_descriptor;
use andromeda_structured_object::{RowCountPolicy, StructuredObjectHeader, StructuredObjectLayout};
use andromeda_types::{ColumnDescriptor, ContractHash, ScalarType, TypeDescriptor};

fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(ScalarType::I64),
        ordinal,
    }
}

fn accepts_structured_object_header(_: andromeda_structured_object::StructuredObjectHeader) {}

fn accepts_structured_object_layout(_: andromeda_structured_object::StructuredObjectLayout) {}

fn accepts_structured_row_count_requirement(_: andromeda_structured_object::RowCountRequirement) {}

#[test]
fn structured_object_owner_paths_keep_type_identity() {
    let fields = vec![column("reservation_id", 0)];
    let layout = StructuredObjectLayout::RowMajor;
    let descriptor_hash = StructuredObjectHeader::compute_descriptor_hash(&fields, layout);
    let header = StructuredObjectHeader {
        name: "Reservation".to_string(),
        contract_hash: ContractHash::test_vector(0x22),
        descriptor_hash,
        fields,
        column_count: 1,
        layout,
        row_count_policy: RowCountPolicy::ExactRequired,
        row_count_exact: Some(1),
        payload_length: 16,
        payload_checksum: None,
        max_payload_length: Some(64),
    };

    accepts_structured_object_header(header.clone());
    accepts_structured_object_layout(layout);
    accepts_structured_row_count_requirement(RowCountRequirement::ExactRequired);

    assert!(header.validate().is_ok());
}

#[test]
fn result_stream_owner_paths_accept_structured_row_count_requirement() {
    let descriptor = ResultStreamDescriptor {
        stream_name: "Reservation".to_string(),
        columns: vec![column("reservation_id", 0)],
        cardinality: ResultCardinality::ExactlyOne,
        row_count_requirement: RowCountRequirement::ExactRequired,
        row_count_exact: Some(1),
        row_count_max: Some(1),
    };

    assert!(descriptor.validate().is_ok());
}

#[test]
fn handwritten_row_count_requirement_is_not_a_generated_protobuf_discriminant() {
    type GeneratedRowCountRequirement = result_stream_descriptor::RowCountRequirement;

    let mapping = [
        (
            RowCountRequirement::UnknownAllowed,
            GeneratedRowCountRequirement::UnknownAllowed,
        ),
        (
            RowCountRequirement::ExactIfKnown,
            GeneratedRowCountRequirement::ExactIfKnown,
        ),
        (
            RowCountRequirement::ExactRequired,
            GeneratedRowCountRequirement::ExactRequired,
        ),
    ];

    assert_eq!(GeneratedRowCountRequirement::Unspecified as i32, 0);
    for (handwritten, generated) in mapping {
        assert_eq!(handwritten as i32 + 1, generated as i32);
        assert_ne!(handwritten as i32, generated as i32);
    }
}
