use super::*;
use std::sync::Arc;

use andromeda_admission::InvocationRequest;
use andromeda_error::AndromedaErrorKind;
use andromeda_procedure_contract::{
    AccessMode, CatalogObjectRef, IsolationPolicy, MultiResultPolicy, ObjectKind,
    ProcedureContractRef, ProcedureErrorPolicy, ProtocolLayoutRef, QualifiedName,
    ResultMetadataPolicy, ResultStreamCardinality, ResultStreamContract, TransactionPolicy,
};
use andromeda_srpl_ir::{
    BoundSrplBodyPlan, BoundSrplOperationPlan, ExecutableProcedurePlan, SrplCatalogBindingEvidence,
};
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, ProcedureId, ScalarType,
    TimestampType, TypeDescriptor,
};

use crate::ProcedureRequestResolver;

struct FakeResolver {
    response: ProcedureResolveResponse,
}

impl ProcedureResolver for FakeResolver {
    fn resolve_procedure(
        &self,
        request: ProcedureResolveRequest,
    ) -> Result<ProcedureResolveResponse, ProcedureResolveError> {
        request.validate_response(&self.response)?;
        Ok(self.response.clone())
    }
}

fn contract_ref() -> ProcedureContractRef {
    ProcedureContractRef {
        procedure_id: ProcedureId::new(7),
        contract_hash: ContractHash::test_vector(0xA7),
        catalog_version: CatalogVersion::new(3),
    }
}

fn procedure_name() -> QualifiedName {
    QualifiedName::parse("Inventory.ReserveStock").unwrap()
}

fn procedure_object() -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(99),
        name: procedure_name(),
        kind: ObjectKind::Procedure,
        catalog_version: CatalogVersion::new(3),
    }
}

fn manifest(contract_ref: ProcedureContractRef) -> SrplProcedureManifest {
    SrplProcedureManifest {
        contract_ref,
        inputs: vec![ColumnDescriptor {
            name: "ProductId".to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal: 0,
        }],
        structured_inputs: vec![QualifiedName::parse("Inventory.ReservationCommand").unwrap()],
        result_streams: vec![ResultStreamContract {
            stream_id: 1,
            name: "Reservation".to_string(),
            columns: vec![ColumnDescriptor {
                name: "ReservedAt".to_string(),
                data_type: TypeDescriptor::required(ScalarType::Timestamp(
                    TimestampType::Transaction,
                )),
                ordinal: 0,
            }],
            cardinality: ResultStreamCardinality::One,
            row_count_exact_required: true,
        }],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Serializable,
            retryable: false,
        },
        required_permissions: vec!["procedure.Execute".to_string()],
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::test_vector(0xD1),
            frame_envelope_hash: ContractHash::test_vector(0xD2),
        },
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: true,
            allowed_error_codes: vec!["OutOfStock".to_string()],
        },
        multi_result_policy: MultiResultPolicy::SingleResultOnly,
    }
}

fn response() -> ProcedureResolveResponse {
    let contract_ref = contract_ref();
    ProcedureResolveResponse {
        procedure_id: contract_ref.procedure_id,
        name: procedure_name(),
        contract_hash: contract_ref.contract_hash,
        catalog_version: contract_ref.catalog_version,
        contract_ref,
        manifest: manifest(contract_ref),
        plan: ExecutableProcedurePlan {
            procedure_name: procedure_name(),
            body: BoundSrplBodyPlan {
                operations: vec![BoundSrplOperationPlan::Raise {
                    ordinal: 0,
                    code: "OutOfStock".to_string(),
                }],
            },
            evidence: SrplCatalogBindingEvidence {
                catalog_version: contract_ref.catalog_version,
                procedure_object: procedure_object(),
                procedure_contract: contract_ref,
                bound_objects: Vec::new(),
            },
        },
    }
}

fn invocation_request() -> InvocationRequest {
    let contract_ref = contract_ref();
    InvocationRequest {
        invocation_id: andromeda_types::InvocationId::new(77),
        procedure: contract_ref,
        expected_binding: None,
        expected_contract_hash: contract_ref.contract_hash,
        catalog_version: contract_ref.catalog_version,
        structured_parameters: Vec::new(),
    }
}

#[test]
fn procedure_resolver_request_validation_rejects_empty_contract_identity() {
    let error = ProcedureResolveRequest::by_procedure_id(
        ProcedureId::new(0),
        ContractHash::zero(),
        CatalogVersion::new(0),
    )
    .unwrap_err();

    assert!(matches!(
        &error,
        ProcedureResolveError::InvalidRequest { .. }
    ));
    assert_eq!(
        error.into_andromeda_error().kind(),
        AndromedaErrorKind::Contract
    );
}

#[test]
fn procedure_resolver_contract_mismatch_maps_to_contract_error() {
    let request = ProcedureResolveRequest::by_procedure_id(
        ProcedureId::new(7),
        ContractHash::test_vector(0xBB),
        CatalogVersion::new(3),
    )
    .unwrap();
    let error = request.validate_response(&response()).unwrap_err();

    assert!(matches!(
        &error,
        ProcedureResolveError::ContractMismatch { .. }
    ));
    assert_eq!(
        error.into_andromeda_error().kind(),
        AndromedaErrorKind::Contract
    );
}

#[test]
fn procedure_resolver_unknown_procedure_maps_to_catalog_error() {
    let request = ProcedureResolveRequest::by_procedure_id(
        ProcedureId::new(8),
        ContractHash::test_vector(0xA7),
        CatalogVersion::new(3),
    )
    .unwrap();
    let error = request.validate_response(&response()).unwrap_err();

    assert!(matches!(
        &error,
        ProcedureResolveError::UnknownProcedure { .. }
    ));
    assert_eq!(
        error.into_andromeda_error().kind(),
        AndromedaErrorKind::Catalog
    );
}

#[test]
fn procedure_resolver_version_mismatch_maps_to_catalog_error() {
    let request = ProcedureResolveRequest::by_procedure_id(
        ProcedureId::new(7),
        ContractHash::test_vector(0xA7),
        CatalogVersion::new(4),
    )
    .unwrap();
    let error = request.validate_response(&response()).unwrap_err();

    assert!(matches!(
        &error,
        ProcedureResolveError::VersionMismatch { .. }
    ));
    assert_eq!(
        error.into_andromeda_error().kind(),
        AndromedaErrorKind::Catalog
    );
}

#[test]
fn procedure_resolver_successful_fake_response_validates() {
    let resolver = FakeResolver {
        response: response(),
    };
    let request = ProcedureResolveRequest::from_contract_ref(contract_ref()).unwrap();
    let resolved = resolver.resolve_procedure(request).unwrap();

    assert_eq!(resolved.contract_ref, contract_ref());
    assert_eq!(resolved.manifest.inputs.len(), 1);
    assert_eq!(resolved.manifest.result_streams[0].name, "Reservation");
    assert_eq!(
        resolved.manifest.transaction_policy.isolation,
        IsolationPolicy::Serializable
    );
}

#[test]
fn procedure_request_resolver_accepts_shared_srpl_resolver() {
    let resolver: Arc<dyn ProcedureResolver> = Arc::new(FakeResolver {
        response: response(),
    });

    resolver
        .resolve_request(&invocation_request())
        .expect("shared SRPL resolver should validate invocation contract");
}

#[test]
fn procedure_request_resolver_rejects_invocation_contract_drift() {
    let resolver: Arc<dyn ProcedureResolver> = Arc::new(FakeResolver {
        response: response(),
    });
    let mut request = invocation_request();
    request.expected_contract_hash = ContractHash::test_vector(0xEE);

    let error = resolver.resolve_request(&request).unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("ContractHash"));
}
