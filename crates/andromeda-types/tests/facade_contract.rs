use andromeda_types::{
    AbsencePolicy, CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, DatabaseId,
    FloatMode, FloatType, InvocationId, NamespaceId, ProcedureId, RequestId, ScalarType, SessionId,
    TextEncoding, TextType, TransactionId, TypeDescriptor,
};

#[test]
fn root_facade_exposes_foundation_identifiers() {
    assert_eq!(CatalogObjectId::new(1).get(), 1);
    assert_eq!(CatalogVersion::new(2).get(), 2);
    assert_eq!(DatabaseId::new(3).get(), 3);
    assert_eq!(InvocationId::new(4).get(), 4);
    assert_eq!(NamespaceId::new(5).get(), 5);
    assert_eq!(ProcedureId::new(6).get(), 6);
    assert_eq!(RequestId::new(7).get(), 7);
    assert_eq!(SessionId::new(8).get(), 8);
    assert_eq!(TransactionId::new(9).get(), 9);
}

#[test]
fn root_facade_exposes_contract_hash_and_descriptors() {
    let hash = ContractHash::new([0xAA; ContractHash::LEN]);
    assert_eq!(hash.as_bytes(), [0xAA; ContractHash::LEN]);

    let descriptor = TypeDescriptor::optional(ScalarType::Text(TextType {
        encoding: TextEncoding::Utf8,
        max_length: Some(64),
        collation: Some("unicode:case-sensitive".to_string()),
    }));

    assert_eq!(descriptor.absence, AbsencePolicy::ExplicitOptional);
    assert!(descriptor.validate().is_ok());
}

#[test]
fn root_facade_exposes_column_descriptor_validation() {
    let column = ColumnDescriptor {
        name: "amount".to_string(),
        data_type: TypeDescriptor::required(ScalarType::Float(FloatType::Custom {
            bits: 64,
            mode: FloatMode::DeterministicAnalytics,
        })),
        ordinal: 0,
    };

    assert!(column.validate().is_ok());
}
