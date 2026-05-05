//! SRPL procedure resolver contract.
//!
//! This module defines the pre-transaction boundary for resolving a procedure
//! identity plus contract expectation into the SRPL artifacts needed for a
//! later invocation.  It intentionally does **not** create transactions,
//! dispatch to a runtime, open storage, speak transport, or accept ad hoc query
//! text.  Implementors are catalog-facing adapters that must either return a
//! fully validated [`ProcedureResolveResponse`] or a typed
//! [`ProcedureResolveError`] before any transaction is created.
//!
//! ## Procedure contract checklist
//!
//! - **Inputs:** [`ProcedureResolveRequest`] carries a target
//!   ([`ProcedureId`] or [`QualifiedName`]), expected [`ContractHash`], and
//!   expected [`CatalogVersion`].  [`SrplProcedureManifest::inputs`] carries the
//!   scalar input columns required by the resolved contract.
//! - **StructuredObjects:** [`SrplProcedureManifest::structured_inputs`] carries
//!   typed catalog names for structured input objects.  The resolver only
//!   reports these names; it does not dereference storage.
//! - **Result streams:** [`SrplProcedureManifest::result_streams`] carries the
//!   catalog result stream contracts that callers must bind before payload
//!   handling.
//! - **Errors:** unknown procedure, contract hash mismatch, catalog version
//!   mismatch, invalid request/response, and resolver rejection are typed as
//!   [`ProcedureResolveError`].  No variant authorizes transaction creation.
//! - **Transaction policy:** [`SrplProcedureManifest::transaction_policy`]
//!   exposes the catalog policy that the transaction layer must later honor.
//!   This module never starts the transaction itself.
//! - **ContractHash inputs:** request validation rejects zero hashes; response
//!   validation requires the response hash, [`ProcedureContractRef`], manifest,
//!   and executable SRPL plan evidence to agree exactly with the request.

use std::collections::BTreeSet;

use andromeda_catalog::{
    MultiResultPolicy, ProcedureContract, ProcedureContractRef, ProcedureErrorPolicy,
    ProtocolLayoutRef, QualifiedName, ResultMetadataPolicy, ResultStreamContract,
    TransactionPolicy,
};
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ColumnDescriptor,
    ContractHash, ProcedureId,
};

use crate::model::ExecutableProcedurePlan;

/// Address of a procedure to resolve before transaction creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcedureResolveTarget {
    ProcedureId(ProcedureId),
    QualifiedName(QualifiedName),
}

impl ProcedureResolveTarget {
    fn validate(&self) -> Result<(), ProcedureResolveError> {
        match self {
            Self::ProcedureId(procedure_id) if procedure_id.get() == 0 => {
                Err(ProcedureResolveError::InvalidRequest {
                    message: "procedure resolver target id must not be zero".to_string(),
                })
            }
            Self::ProcedureId(_) | Self::QualifiedName(_) => Ok(()),
        }
    }

    fn matches_response(&self, response: &ProcedureResolveResponse) -> bool {
        match self {
            Self::ProcedureId(procedure_id) => *procedure_id == response.procedure_id,
            Self::QualifiedName(name) => name == &response.name,
        }
    }
}

/// Pre-transaction procedure resolution request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureResolveRequest {
    pub target: ProcedureResolveTarget,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
}

impl ProcedureResolveRequest {
    pub fn by_procedure_id(
        procedure_id: ProcedureId,
        contract_hash: ContractHash,
        catalog_version: CatalogVersion,
    ) -> Result<Self, ProcedureResolveError> {
        let request = Self {
            target: ProcedureResolveTarget::ProcedureId(procedure_id),
            contract_hash,
            catalog_version,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn by_qualified_name(
        name: QualifiedName,
        contract_hash: ContractHash,
        catalog_version: CatalogVersion,
    ) -> Result<Self, ProcedureResolveError> {
        let request = Self {
            target: ProcedureResolveTarget::QualifiedName(name),
            contract_hash,
            catalog_version,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn from_contract_ref(
        contract_ref: ProcedureContractRef,
    ) -> Result<Self, ProcedureResolveError> {
        contract_ref
            .validate()
            .map_err(ProcedureResolveError::invalid_request)?;
        Self::by_procedure_id(
            contract_ref.procedure_id,
            contract_ref.contract_hash,
            contract_ref.catalog_version,
        )
    }

    pub fn validate(&self) -> Result<(), ProcedureResolveError> {
        self.target.validate()?;
        if self.contract_hash.is_zero() {
            return Err(ProcedureResolveError::InvalidRequest {
                message: "procedure resolver request contract hash must not be zero".to_string(),
            });
        }
        if self.catalog_version.get() == 0 {
            return Err(ProcedureResolveError::InvalidRequest {
                message: "procedure resolver request catalog version must not be zero".to_string(),
            });
        }
        Ok(())
    }

    pub fn expected_contract_ref(&self) -> Option<ProcedureContractRef> {
        match self.target {
            ProcedureResolveTarget::ProcedureId(procedure_id) => Some(ProcedureContractRef {
                procedure_id,
                contract_hash: self.contract_hash,
                catalog_version: self.catalog_version,
            }),
            ProcedureResolveTarget::QualifiedName(_) => None,
        }
    }

    /// Validate that a resolver response exactly satisfies this request.
    pub fn validate_response(
        &self,
        response: &ProcedureResolveResponse,
    ) -> Result<(), ProcedureResolveError> {
        self.validate()?;
        response.validate()?;

        if !self.target.matches_response(response) {
            return Err(ProcedureResolveError::UnknownProcedure {
                target: self.target.clone(),
            });
        }
        if response.catalog_version != self.catalog_version {
            return Err(ProcedureResolveError::VersionMismatch {
                requested: self.catalog_version,
                actual: response.catalog_version,
            });
        }
        if response.contract_hash != self.contract_hash {
            return Err(ProcedureResolveError::ContractMismatch {
                procedure_id: response.procedure_id,
                expected: self.contract_hash,
                actual: response.contract_hash,
            });
        }

        Ok(())
    }
}

/// Resolver-visible manifest for a resolved SRPL procedure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplProcedureManifest {
    pub contract_ref: ProcedureContractRef,
    pub inputs: Vec<ColumnDescriptor>,
    pub structured_inputs: Vec<QualifiedName>,
    pub result_streams: Vec<ResultStreamContract>,
    pub transaction_policy: TransactionPolicy,
    pub required_permissions: Vec<String>,
    pub protocol_layout: ProtocolLayoutRef,
    pub result_metadata_policy: ResultMetadataPolicy,
    pub error_policy: ProcedureErrorPolicy,
    pub multi_result_policy: MultiResultPolicy,
}

impl SrplProcedureManifest {
    pub fn from_contract(contract: &ProcedureContract) -> AndromedaResult<Self> {
        contract.validate()?;
        Ok(Self {
            contract_ref: contract.as_ref(),
            inputs: contract.inputs.clone(),
            structured_inputs: contract.structured_inputs.clone(),
            result_streams: contract.result_streams.clone(),
            transaction_policy: contract.transaction_policy,
            required_permissions: contract.required_permissions.clone(),
            protocol_layout: contract.protocol_layout,
            result_metadata_policy: contract.result_metadata_policy,
            error_policy: contract.error_policy.clone(),
            multi_result_policy: contract.multi_result_policy,
        })
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.contract_ref.validate()?;
        validate_dense_columns_allow_empty(&self.inputs, "procedure resolver input columns")?;
        self.protocol_layout.validate()?;
        self.error_policy.validate()?;
        if self.required_permissions.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                "procedure resolver manifest must declare required permissions",
            ));
        }
        let mut permissions = BTreeSet::new();
        for permission in &self.required_permissions {
            if permission.trim().is_empty() {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Security,
                    "procedure resolver manifest permissions must not be empty",
                ));
            }
            if !permissions.insert(permission.as_str()) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Security,
                    "procedure resolver manifest permissions must be unique",
                ));
            }
        }

        let mut structured_inputs = BTreeSet::new();
        for structured_input in &self.structured_inputs {
            if !structured_inputs.insert(structured_input) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure resolver manifest structured inputs must be unique",
                ));
            }
        }

        let mut stream_ids = BTreeSet::new();
        let mut stream_names = BTreeSet::new();
        for stream in &self.result_streams {
            stream.validate()?;
            if !stream_ids.insert(stream.stream_id) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure resolver manifest result stream ids must be unique",
                ));
            }
            if !stream_names.insert(stream.name.as_str()) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure resolver manifest result stream names must be unique",
                ));
            }
        }
        if self.multi_result_policy == MultiResultPolicy::SingleResultOnly
            && self.result_streams.len() > 1
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure resolver manifest multi-result policy allows only one stream",
            ));
        }

        Ok(())
    }
}

/// Successful pre-transaction resolution outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureResolveResponse {
    pub procedure_id: ProcedureId,
    pub name: QualifiedName,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
    pub contract_ref: ProcedureContractRef,
    pub manifest: SrplProcedureManifest,
    pub plan: ExecutableProcedurePlan,
}

impl ProcedureResolveResponse {
    pub fn validate(&self) -> Result<(), ProcedureResolveError> {
        self.contract_ref
            .validate()
            .map_err(ProcedureResolveError::invalid_response)?;
        self.manifest
            .validate()
            .map_err(ProcedureResolveError::invalid_response)?;
        self.plan
            .validate()
            .map_err(ProcedureResolveError::invalid_response)?;

        if self.procedure_id.get() == 0 {
            return Err(ProcedureResolveError::InvalidResponse {
                message: "procedure resolver response id must not be zero".to_string(),
            });
        }
        if self.contract_hash.is_zero() {
            return Err(ProcedureResolveError::InvalidResponse {
                message: "procedure resolver response contract hash must not be zero".to_string(),
            });
        }
        if self.catalog_version.get() == 0 {
            return Err(ProcedureResolveError::InvalidResponse {
                message: "procedure resolver response catalog version must not be zero".to_string(),
            });
        }
        if self.contract_ref.procedure_id != self.procedure_id
            || self.contract_ref.contract_hash != self.contract_hash
            || self.contract_ref.catalog_version != self.catalog_version
        {
            return Err(ProcedureResolveError::InvalidResponse {
                message: "procedure resolver response identities must match ProcedureContractRef"
                    .to_string(),
            });
        }
        if self.manifest.contract_ref != self.contract_ref {
            return Err(ProcedureResolveError::InvalidResponse {
                message: "procedure resolver manifest must carry the resolved contract ref"
                    .to_string(),
            });
        }
        if self.plan.procedure_name != self.name {
            return Err(ProcedureResolveError::InvalidResponse {
                message: "procedure resolver plan name must match the resolved procedure"
                    .to_string(),
            });
        }
        if self.plan.evidence.procedure_contract != self.contract_ref {
            return Err(ProcedureResolveError::InvalidResponse {
                message: "procedure resolver plan evidence must match the resolved contract ref"
                    .to_string(),
            });
        }

        Ok(())
    }
}

/// Typed failures from the pre-transaction resolver boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcedureResolveError {
    InvalidRequest {
        message: String,
    },
    UnknownProcedure {
        target: ProcedureResolveTarget,
    },
    ContractMismatch {
        procedure_id: ProcedureId,
        expected: ContractHash,
        actual: ContractHash,
    },
    VersionMismatch {
        requested: CatalogVersion,
        actual: CatalogVersion,
    },
    InvalidResponse {
        message: String,
    },
    ResolverRejected {
        message: String,
    },
}

impl ProcedureResolveError {
    pub fn invalid_request(error: AndromedaError) -> Self {
        Self::InvalidRequest {
            message: error.message().to_string(),
        }
    }

    pub fn invalid_response(error: AndromedaError) -> Self {
        Self::InvalidResponse {
            message: error.message().to_string(),
        }
    }

    pub fn into_andromeda_error(self) -> AndromedaError {
        match self {
            Self::InvalidRequest { message } | Self::InvalidResponse { message } => {
                AndromedaError::new(AndromedaErrorKind::Contract, message)
            }
            Self::UnknownProcedure { .. } => AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "procedure resolver found no matching procedure",
            ),
            Self::ContractMismatch { .. } => AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure resolver contract hash mismatch",
            ),
            Self::VersionMismatch { .. } => AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "procedure resolver catalog version mismatch",
            ),
            Self::ResolverRejected { message } => {
                AndromedaError::new(AndromedaErrorKind::Catalog, message)
            }
        }
    }
}

/// Pre-transaction resolver for SRPL executable plans and manifests.
pub trait ProcedureResolver {
    fn resolve_procedure(
        &self,
        request: ProcedureResolveRequest,
    ) -> Result<ProcedureResolveResponse, ProcedureResolveError>;
}

fn validate_dense_columns_allow_empty(
    columns: &[ColumnDescriptor],
    context: &str,
) -> AndromedaResult<()> {
    let mut names = BTreeSet::new();
    for (expected_ordinal, column) in columns.iter().enumerate() {
        column.validate()?;
        if !names.insert(column.name.as_str()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                format!("{context} names must be unique"),
            ));
        }
        if column.ordinal != expected_ordinal as u32 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                format!("{context} must be dense and zero-based"),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_catalog::{
        AccessMode, CatalogObjectRef, IsolationPolicy, ObjectKind, ResultStreamContract,
        TransactionPolicy,
    };
    use andromeda_core::{CatalogObjectId, ScalarType, TimestampType, TypeDescriptor};

    use crate::model::{BoundSrplBodyPlan, BoundSrplOperationPlan, SrplCatalogBindingEvidence};

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
}
