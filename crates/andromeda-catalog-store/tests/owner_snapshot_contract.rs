#![forbid(unsafe_code)]

use andromeda_catalog_store::{
    CatalogDefinition, CatalogDurabilityMarker, CatalogMutationDelta, CatalogMutationDurability,
    CatalogObjectRef, CatalogPublicationCommitEvidence, CatalogPublicationPlan,
    CatalogPublicationReceipt, CatalogPublicationSemantics, CatalogSnapshot,
    CatalogSnapshotMutationPlan, CatalogSnapshotPublication, CatalogStoreMutationKind,
    CatalogStoreWalAppend, CatalogStoreWalAppendSequenceError, ObjectKind, QualifiedName,
    StructuredObjectDefinition, TableDefinition, validate_catalog_store_wal_append_sequence,
};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_procedure_contract::{
    AccessMode, CompatibilityPolicy, IsolationPolicy, MultiResultPolicy,
    ProcedureContractCandidate, ProcedureErrorPolicy, ProtocolLayoutRef, ResultMetadataPolicy,
    StatsVersion, TransactionPolicy,
};
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, DatabaseId, NamespaceId,
    ProcedureId, ScalarType, TypeDescriptor,
};

const DATABASE_ID: DatabaseId = DatabaseId::new(1);
const NAMESPACE_ID: NamespaceId = NamespaceId::new(2);

type TestReceipt = CatalogPublicationReceipt<u64, [u8; 32], [u8; 32]>;

#[derive(Debug, Clone)]
struct TestPlan {
    database_id: DatabaseId,
    namespace_id: NamespaceId,
    previous_version: CatalogVersion,
    next_version: CatalogVersion,
    deltas: Vec<CatalogMutationDelta<CatalogObjectRef>>,
}

impl CatalogSnapshotMutationPlan for TestPlan {
    type LifecycleTarget = CatalogObjectRef;

    fn database_id(&self) -> DatabaseId {
        self.database_id
    }

    fn namespace_id(&self) -> NamespaceId {
        self.namespace_id
    }

    fn previous_version(&self) -> CatalogVersion {
        self.previous_version
    }

    fn next_version(&self) -> CatalogVersion {
        self.next_version
    }

    fn publication_semantics(&self) -> CatalogPublicationSemantics {
        CatalogPublicationSemantics::DurablePublicationExternal
    }

    fn is_monotonic(&self) -> bool {
        self.next_version.get() > self.previous_version.get()
    }

    fn deltas(&self) -> &[CatalogMutationDelta<Self::LifecycleTarget>] {
        &self.deltas
    }
}

impl CatalogPublicationPlan<u64, [u8; 32], [u8; 32]> for TestPlan {
    fn batch_id(&self) -> u64 {
        1001
    }

    fn database_id(&self) -> DatabaseId {
        self.database_id
    }

    fn namespace_id(&self) -> NamespaceId {
        self.namespace_id
    }

    fn previous_version(&self) -> CatalogVersion {
        self.previous_version
    }

    fn next_version(&self) -> CatalogVersion {
        self.next_version
    }

    fn source_hash(&self) -> [u8; 32] {
        [0xA5; 32]
    }

    fn dependency_graph_hash(&self) -> [u8; 32] {
        [0xC3; 32]
    }

    fn record_count(&self) -> usize {
        self.deltas.len() + 2
    }

    fn publication_semantics(&self) -> CatalogPublicationSemantics {
        CatalogPublicationSemantics::DurablePublicationExternal
    }

    fn is_monotonic(&self) -> bool {
        self.next_version.get() > self.previous_version.get()
    }
}

#[derive(Debug, Clone, Copy)]
struct TestCommitEvidence {
    durability: CatalogMutationDurability,
    record_count: usize,
}

impl CatalogPublicationCommitEvidence<TestPlan> for TestCommitEvidence {
    fn validate_for_publication_plan(&self, plan: &TestPlan) -> AndromedaResult<()> {
        if self.record_count != plan.record_count() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "test catalog publication record count mismatch",
            ));
        }

        self.durability.validate()
    }

    fn durable_lsn(&self) -> Option<u64> {
        self.durability.durable_lsn()
    }

    fn durable_evidence_marker(&self) -> Option<CatalogDurabilityMarker> {
        self.durability.durable_marker()
    }

    fn record_count(&self) -> usize {
        self.record_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TestRecordKind {
    Begin,
    Apply,
    Commit,
}

impl CatalogStoreMutationKind for TestRecordKind {
    fn is_commit_record(self) -> bool {
        self == Self::Commit
    }
}

fn version(value: u64) -> CatalogVersion {
    CatalogVersion::new(value)
}

fn snapshot_at(catalog_version: u64) -> CatalogSnapshot<TestReceipt> {
    CatalogSnapshot::empty(DATABASE_ID, NAMESPACE_ID, version(catalog_version))
}

fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(ScalarType::I64),
        ordinal,
    }
}

fn object(id: u64, name: &str, kind: ObjectKind, catalog_version: u64) -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(id),
        name: QualifiedName::parse(name).unwrap(),
        kind,
        catalog_version: version(catalog_version),
    }
}

fn table(id: u64, name: &str, catalog_version: u64) -> CatalogDefinition {
    CatalogDefinition::Table(TableDefinition {
        object: object(id, name, ObjectKind::Table, catalog_version),
        columns: vec![column("ProductId", 0)],
    })
}

fn structured_object(id: u64, name: &str, catalog_version: u64) -> CatalogDefinition {
    CatalogDefinition::StructuredObject(StructuredObjectDefinition {
        object: object(id, name, ObjectKind::StructuredObject, catalog_version),
        fields: vec![column("ProductId", 0)],
        unique_by: vec!["ProductId".to_string()],
    })
}

fn procedure(
    id: u64,
    name: &str,
    catalog_version: u64,
    structured_inputs: Vec<QualifiedName>,
) -> CatalogDefinition {
    CatalogDefinition::Procedure(
        ProcedureContractCandidate {
            object: object(id, name, ObjectKind::Procedure, catalog_version),
            procedure_id: ProcedureId::new(id),
            stats_version: StatsVersion::new(1),
            protocol_layout: ProtocolLayoutRef {
                descriptor_set_hash: ContractHash::test_vector(0xA1),
                frame_envelope_hash: ContractHash::test_vector(0xA2),
            },
            inputs: vec![column("ProductId", 0)],
            structured_inputs,
            result_streams: Vec::new(),
            required_permissions: vec!["Inventory.ReserveStock.Execute".to_string()],
            transaction_policy: TransactionPolicy {
                access_mode: AccessMode::ReadWrite,
                isolation: IsolationPolicy::Serializable,
                retryable: false,
            },
            compatibility_policy: CompatibilityPolicy::ExactHash,
            result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
            error_policy: ProcedureErrorPolicy {
                rollback_on_error: true,
                allowed_error_codes: vec!["InsufficientStock".to_string()],
            },
            multi_result_policy: MultiResultPolicy::SingleResultOnly,
        }
        .materialize()
        .unwrap(),
    )
}

fn create_plan(previous_version: u64, definitions: Vec<CatalogDefinition>) -> TestPlan {
    let next_version = version(previous_version + 1);
    TestPlan {
        database_id: DATABASE_ID,
        namespace_id: NAMESPACE_ID,
        previous_version: version(previous_version),
        next_version,
        deltas: definitions
            .into_iter()
            .enumerate()
            .map(|(index, definition)| {
                CatalogMutationDelta::create(index, next_version, definition)
            })
            .collect(),
    }
}

fn deprecate_plan(previous_version: u64, targets: Vec<CatalogObjectRef>) -> TestPlan {
    let next_version = version(previous_version + 1);
    TestPlan {
        database_id: DATABASE_ID,
        namespace_id: NAMESPACE_ID,
        previous_version: version(previous_version),
        next_version,
        deltas: targets
            .into_iter()
            .enumerate()
            .map(|(index, target)| CatalogMutationDelta::deprecate(index, next_version, target))
            .collect(),
    }
}

#[test]
fn catalog_store_owner_api_has_no_storage_or_exec_coupling() {
    let manifest = include_str!("../Cargo.toml");

    assert!(!manifest.contains("andromeda-storage"));
    assert!(!manifest.contains("andromeda-exec"));
    assert!(matches!(
        snapshot_at(1).publication,
        CatalogSnapshotPublication::InMemoryOnly
    ));
}

#[test]
fn owner_snapshot_rejects_catalog_identity_mismatch() {
    let mut wrong_database = create_plan(10, vec![table(1, "Inventory.Product", 11)]);
    wrong_database.database_id = DatabaseId::new(DATABASE_ID.get() + 100);

    let error = snapshot_at(10)
        .apply_mutation_plan(&wrong_database)
        .unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("database id"));

    let mut wrong_namespace = create_plan(10, vec![table(1, "Inventory.Product", 11)]);
    wrong_namespace.namespace_id = NamespaceId::new(NAMESPACE_ID.get() + 100);

    let error = snapshot_at(10)
        .apply_mutation_plan(&wrong_namespace)
        .unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("namespace id"));
}

#[test]
fn owner_snapshot_advances_applied_state_without_durable_publication() {
    let mut snapshot = snapshot_at(10);
    let report = snapshot
        .apply_mutation_plan(&create_plan(10, vec![table(1, "Inventory.Product", 11)]))
        .unwrap();

    assert_eq!(snapshot.version, version(11));
    assert_eq!(snapshot.visible_version(), version(10));
    assert_eq!(snapshot.staged_in_memory_version(), Some(version(11)));
    assert_eq!(report.previous_version, version(10));
    assert_eq!(report.next_version, version(11));
    assert!(!report.durable_publication_performed);
    assert!(matches!(
        snapshot.publication,
        CatalogSnapshotPublication::InMemoryOnly
    ));
}

#[test]
fn owner_snapshot_publishes_only_after_durable_evidence() {
    let mut snapshot = snapshot_at(10);
    let plan = create_plan(10, vec![table(1, "Inventory.Product", 11)]);
    let receipt = snapshot
        .publish_durable_mutation_plan(
            &plan,
            TestCommitEvidence {
                durability: CatalogMutationDurability::StorageWal {
                    commit_lsn: 77,
                    durable_lsn: 80,
                },
                record_count: plan.record_count(),
            },
        )
        .unwrap();

    assert_eq!(snapshot.version, version(11));
    assert_eq!(snapshot.visible_version(), version(11));
    assert_eq!(snapshot.visible_publication_receipt(), Some(receipt));
    assert_eq!(receipt.durable_lsn, Some(80));
    assert_eq!(receipt.record_count, plan.record_count());
    assert!(matches!(
        snapshot.publication,
        CatalogSnapshotPublication::Durable(published) if published == receipt
    ));
}

#[test]
fn owner_snapshot_rejects_non_durable_publication_evidence() {
    let mut snapshot = snapshot_at(10);
    let plan = create_plan(10, vec![table(1, "Inventory.Product", 11)]);

    let error = snapshot
        .publish_durable_mutation_plan(
            &plan,
            TestCommitEvidence {
                durability: CatalogMutationDurability::StorageWal {
                    commit_lsn: 77,
                    durable_lsn: 76,
                },
                record_count: plan.record_count(),
            },
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("durable LSN"));
    assert_eq!(snapshot.version, version(10));
}

#[test]
fn owner_snapshot_validates_external_structured_input_dependencies() {
    let missing_error = snapshot_at(10)
        .apply_mutation_plan(&create_plan(
            10,
            vec![procedure(
                2,
                "Inventory.ReserveStock",
                11,
                vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
            )],
        ))
        .unwrap_err();
    assert_eq!(missing_error.kind(), AndromedaErrorKind::Catalog);
    assert!(missing_error.message().contains("dependency"));
    assert!(missing_error.message().contains("missing"));

    let mut snapshot = snapshot_at(10);
    snapshot
        .apply_mutation_plan(&create_plan(
            10,
            vec![structured_object(1, "Inventory.StockRequest", 11)],
        ))
        .unwrap();
    snapshot
        .apply_mutation_plan(&create_plan(
            11,
            vec![procedure(
                2,
                "Inventory.ReserveStock",
                12,
                vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
            )],
        ))
        .unwrap();

    assert_eq!(snapshot.version, version(12));
    assert!(
        snapshot.applied_contains_name(&QualifiedName::parse("Inventory.ReserveStock").unwrap())
    );
}

#[test]
fn owner_snapshot_rejects_deprecation_with_active_dependents() {
    let mut snapshot = snapshot_at(10);
    snapshot
        .apply_mutation_plan(&create_plan(
            10,
            vec![structured_object(1, "Inventory.StockRequest", 11)],
        ))
        .unwrap();
    snapshot
        .apply_mutation_plan(&create_plan(
            11,
            vec![procedure(
                2,
                "Inventory.ReserveStock",
                12,
                vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
            )],
        ))
        .unwrap();

    let error = snapshot
        .apply_mutation_plan(&deprecate_plan(
            12,
            vec![object(
                1,
                "Inventory.StockRequest",
                ObjectKind::StructuredObject,
                11,
            )],
        ))
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("active dependents"));
    assert_eq!(snapshot.version, version(12));
}

#[test]
fn owner_snapshot_allows_joint_deprecation_of_dependency_and_dependent() {
    let mut snapshot = snapshot_at(10);
    snapshot
        .apply_mutation_plan(&create_plan(
            10,
            vec![structured_object(1, "Inventory.StockRequest", 11)],
        ))
        .unwrap();
    snapshot
        .apply_mutation_plan(&create_plan(
            11,
            vec![procedure(
                2,
                "Inventory.ReserveStock",
                12,
                vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
            )],
        ))
        .unwrap();

    snapshot
        .apply_mutation_plan(&deprecate_plan(
            12,
            vec![
                object(2, "Inventory.ReserveStock", ObjectKind::Procedure, 12),
                object(
                    1,
                    "Inventory.StockRequest",
                    ObjectKind::StructuredObject,
                    11,
                ),
            ],
        ))
        .unwrap();

    assert_eq!(snapshot.version, version(13));
    assert!(!snapshot.applied_is_active_object(CatalogObjectId::new(1)));
    assert!(!snapshot.applied_is_active_object(CatalogObjectId::new(2)));
}

#[test]
fn owner_snapshot_rejects_new_dependent_when_dependency_is_deprecated() {
    let mut snapshot = snapshot_at(10);
    snapshot
        .apply_mutation_plan(&create_plan(
            10,
            vec![structured_object(1, "Inventory.StockRequest", 11)],
        ))
        .unwrap();

    let error = snapshot
        .apply_mutation_plan(&TestPlan {
            database_id: DATABASE_ID,
            namespace_id: NAMESPACE_ID,
            previous_version: version(11),
            next_version: version(12),
            deltas: vec![
                CatalogMutationDelta::create(
                    0,
                    version(12),
                    procedure(
                        2,
                        "Inventory.ReserveStock",
                        12,
                        vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                    ),
                ),
                CatalogMutationDelta::deprecate(
                    1,
                    version(12),
                    object(
                        1,
                        "Inventory.StockRequest",
                        ObjectKind::StructuredObject,
                        11,
                    ),
                ),
            ],
        })
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("active dependents"));
}

#[test]
fn owner_store_wal_append_sequence_requires_monotonic_commit_terminal_records() {
    let expected = [
        TestRecordKind::Begin,
        TestRecordKind::Apply,
        TestRecordKind::Commit,
    ];
    let valid = [
        CatalogStoreWalAppend {
            kind: TestRecordKind::Begin,
            lsn: 70,
        },
        CatalogStoreWalAppend {
            kind: TestRecordKind::Apply,
            lsn: 71,
        },
        CatalogStoreWalAppend {
            kind: TestRecordKind::Commit,
            lsn: 72,
        },
    ];
    assert!(validate_catalog_store_wal_append_sequence(&valid, &expected).is_ok());

    let non_monotonic = [
        CatalogStoreWalAppend {
            kind: TestRecordKind::Begin,
            lsn: 70,
        },
        CatalogStoreWalAppend {
            kind: TestRecordKind::Apply,
            lsn: 70,
        },
        CatalogStoreWalAppend {
            kind: TestRecordKind::Commit,
            lsn: 72,
        },
    ];
    assert_eq!(
        validate_catalog_store_wal_append_sequence(&non_monotonic, &expected),
        Err(CatalogStoreWalAppendSequenceError::NonIncreasingLsn { index: 1 })
    );

    let missing_commit = [
        CatalogStoreWalAppend {
            kind: TestRecordKind::Begin,
            lsn: 70,
        },
        CatalogStoreWalAppend {
            kind: TestRecordKind::Apply,
            lsn: 71,
        },
        CatalogStoreWalAppend {
            kind: TestRecordKind::Apply,
            lsn: 72,
        },
    ];
    assert_eq!(
        validate_catalog_store_wal_append_sequence(&missing_commit, &expected),
        Err(CatalogStoreWalAppendSequenceError::MissingCommitRecord)
    );
}
