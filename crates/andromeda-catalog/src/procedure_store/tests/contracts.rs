use super::*;

#[test]
fn store_round_trips_a_validated_procedure_contract() {
    let mut contract = crate::ProcedureContract {
        object: CatalogObjectRef {
            object_id: CatalogObjectId::new(11),
            name: QualifiedName::parse("Inventory.ReserveStock").unwrap(),
            kind: ObjectKind::Procedure,
            catalog_version: CatalogVersion::new(7),
        },
        procedure_id: ProcedureId::new(1),
        contract_hash: ContractHash::test_vector(42),
        stats_version: StatsVersion::new(1),
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::test_vector(101),
            frame_envelope_hash: ContractHash::test_vector(202),
        },
        inputs: Vec::new(),
        structured_inputs: Vec::new(),
        result_streams: Vec::new(),
        required_permissions: vec!["execute_procedure".to_string()],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Snapshot,
            retryable: false,
        },
        compatibility_policy: CompatibilityPolicy::ExactHash,
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: true,
            allowed_error_codes: Vec::new(),
        },
        multi_result_policy: MultiResultPolicy::SingleResultOnly,
    };
    contract.contract_hash = contract.canonical_hash();
    let expected_hash = contract.contract_hash;

    let entry = ProcedureStoreEntry::from_contract(&contract).unwrap();
    let mut store = ProcedureStore::new();
    assert_eq!(
        store.register(entry).unwrap(),
        ProcedureRegistration::Inserted
    );
    assert_eq!(
        store.get(ProcedureId::new(1)).unwrap().contract_hash(),
        expected_hash
    );
}

#[test]
fn store_entry_from_contract_rejects_stale_canonical_contract_hash() {
    let mut contract = crate::ProcedureContract {
        object: CatalogObjectRef {
            object_id: CatalogObjectId::new(11),
            name: QualifiedName::parse("Inventory.ReserveStock").unwrap(),
            kind: ObjectKind::Procedure,
            catalog_version: CatalogVersion::new(7),
        },
        procedure_id: ProcedureId::new(1),
        contract_hash: ContractHash::test_vector(42),
        stats_version: StatsVersion::new(1),
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::test_vector(101),
            frame_envelope_hash: ContractHash::test_vector(202),
        },
        inputs: Vec::new(),
        structured_inputs: Vec::new(),
        result_streams: Vec::new(),
        required_permissions: vec!["execute_procedure".to_string()],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Snapshot,
            retryable: false,
        },
        compatibility_policy: CompatibilityPolicy::ExactHash,
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: true,
            allowed_error_codes: Vec::new(),
        },
        multi_result_policy: MultiResultPolicy::SingleResultOnly,
    };
    contract.contract_hash = contract.canonical_hash();
    contract.stats_version = StatsVersion::new(2);

    let err = ProcedureStoreEntry::from_contract(&contract).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("canonical contract shape"));
}
