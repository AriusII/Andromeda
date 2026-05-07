//! Core contract types: enums, structs, and their basic constructors and validation.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, ColumnDescriptor, ContractHash, ProcedureId};

use crate::{
    CatalogObjectRef, ObjectKind,
    names::QualifiedName,
    objects::{validate_columns, validate_columns_allow_empty},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcedureContractRef {
    pub procedure_id: ProcedureId,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
}

impl ProcedureContractRef {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.procedure_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure id must not be zero",
            ));
        }

        if self.catalog_version.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure catalog version must not be zero",
            ));
        }

        if self.contract_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure contract hash must not be zero",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationPolicy {
    Snapshot,
    Serializable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionPolicy {
    pub access_mode: AccessMode,
    pub isolation: IsolationPolicy,
    pub retryable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatibilityPolicy {
    AdditiveOnly,
    ExactHash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractCompatibilityDiagnostic {
    pub compatible: bool,
    pub messages: Vec<String>,
}

impl ContractCompatibilityDiagnostic {
    pub fn compatible() -> Self {
        Self {
            compatible: true,
            messages: Vec::new(),
        }
    }

    pub fn incompatible(messages: Vec<String>) -> Self {
        Self {
            compatible: false,
            messages,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultStreamCardinality {
    One,
    OptionalOne,
    Many,
    NonEmptyMany,
}

impl ResultStreamCardinality {
    pub const fn legacy_row_count_exact_required(self) -> bool {
        matches!(self, Self::One | Self::NonEmptyMany)
    }

    pub const fn min_row_count(self) -> u64 {
        match self {
            Self::One | Self::NonEmptyMany => 1,
            Self::OptionalOne | Self::Many => 0,
        }
    }

    pub const fn intrinsic_max_row_count(self) -> Option<u64> {
        match self {
            Self::One | Self::OptionalOne => Some(1),
            Self::Many | Self::NonEmptyMany => None,
        }
    }

    pub const fn stable_tag(self) -> u8 {
        match self {
            Self::One => 0,
            Self::OptionalOne => 1,
            Self::Many => 2,
            Self::NonEmptyMany => 3,
        }
    }

    pub const fn from_legacy_row_count_exact_required(required: bool) -> Self {
        if required { Self::One } else { Self::Many }
    }

    pub fn from_stable_tag(tag: u8) -> AndromedaResult<Self> {
        match tag {
            0 => Ok(Self::One),
            1 => Ok(Self::OptionalOne),
            2 => Ok(Self::Many),
            3 => Ok(Self::NonEmptyMany),
            _ => Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "unknown result stream cardinality tag",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultStreamContract {
    pub stream_id: u64,
    pub name: String,
    pub columns: Vec<ColumnDescriptor>,
    pub cardinality: ResultStreamCardinality,
    /// Legacy v0/v1 compatibility projection. New code must use
    /// `cardinality`; validation requires this flag to remain the deterministic
    /// projection of the full cardinality.
    pub row_count_exact_required: bool,
}

impl ResultStreamContract {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.stream_id == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream id must not be zero",
            ));
        }

        if self.name.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream name must not be empty",
            ));
        }

        validate_columns(&self.columns)?;

        if self.row_count_exact_required != self.cardinality.legacy_row_count_exact_required() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream legacy row-count flag must match full cardinality",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StatsVersion(u64);

impl StatsVersion {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl From<u64> for StatsVersion {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

/// Digest-derived identity of the policy surface bound to a procedure contract.
///
/// `PolicyVersion` is computed from the canonical SHA-256 digest of every
/// policy field that governs how an invocation may execute (transaction,
/// compatibility, error, multi-result, result-metadata, required permissions,
/// stats version).  Two contracts that differ only in non-policy fields share
/// the same `PolicyVersion`; any change to a policy-relevant field changes it.
///
/// Bindings (see [`ProcedureContractBinding`]) carry `PolicyVersion` alongside
/// `ContractHash`, `CatalogVersion`, and `StatsVersion` so consumers can
/// validate policy-aware compatibility independently of the full contract
/// shape hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PolicyVersion([u8; ContractHash::LEN]);

impl PolicyVersion {
    pub const LEN: usize = ContractHash::LEN;

    pub const fn new(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub const fn zero() -> Self {
        Self([0; Self::LEN])
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }
}

/// Full procedure binding evidence: the four identities that the SRPL
/// specification requires every procedure invocation to bind against.
///
/// Unlike [`ProcedureContractRef`] (which is the legacy 3-tuple kept for
/// backward compatibility with the exec engine), `ProcedureContractBinding`
/// also carries the `StatsVersion` and `PolicyVersion` so binders can detect
/// policy or statistics drift even when the canonical contract hash is
/// otherwise unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcedureContractBinding {
    pub procedure_id: ProcedureId,
    pub catalog_version: CatalogVersion,
    pub contract_hash: ContractHash,
    pub stats_version: StatsVersion,
    pub policy_version: PolicyVersion,
}

impl ProcedureContractBinding {
    pub fn new(
        procedure_id: ProcedureId,
        catalog_version: CatalogVersion,
        contract_hash: ContractHash,
        stats_version: StatsVersion,
        policy_version: PolicyVersion,
    ) -> AndromedaResult<Self> {
        let binding = Self {
            procedure_id,
            catalog_version,
            contract_hash,
            stats_version,
            policy_version,
        };
        binding.validate()?;
        Ok(binding)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.procedure_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure binding id must not be zero",
            ));
        }
        if self.catalog_version.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure binding catalog version must not be zero",
            ));
        }
        if self.contract_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure binding contract hash must not be zero",
            ));
        }
        if self.stats_version.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure binding stats version must not be zero",
            ));
        }
        if self.policy_version.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure binding policy version must not be zero",
            ));
        }

        Ok(())
    }

    pub fn as_legacy_ref(&self) -> ProcedureContractRef {
        ProcedureContractRef {
            procedure_id: self.procedure_id,
            contract_hash: self.contract_hash,
            catalog_version: self.catalog_version,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolLayoutRef {
    pub descriptor_set_hash: ContractHash,
    pub frame_envelope_hash: ContractHash,
}

impl ProtocolLayoutRef {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.descriptor_set_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure protocol descriptor set hash must not be zero",
            ));
        }

        if self.frame_envelope_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure protocol frame envelope hash must not be zero",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultMetadataPolicy {
    RequireBeforePayload,
    AllowStreamingUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureErrorPolicy {
    pub rollback_on_error: bool,
    pub allowed_error_codes: Vec<String>,
}

impl ProcedureErrorPolicy {
    pub fn validate(&self) -> AndromedaResult<()> {
        let mut codes = std::collections::BTreeSet::new();
        for code in &self.allowed_error_codes {
            if code.trim().is_empty() {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure error codes must not be empty",
                ));
            }

            if !codes.insert(code.as_str()) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure error codes must be unique",
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiResultPolicy {
    SingleResultOnly,
    MultipleResultStreamsAllowed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureContract {
    pub object: CatalogObjectRef,
    pub procedure_id: ProcedureId,
    pub contract_hash: ContractHash,
    pub stats_version: StatsVersion,
    pub protocol_layout: ProtocolLayoutRef,
    pub inputs: Vec<ColumnDescriptor>,
    pub structured_inputs: Vec<QualifiedName>,
    pub result_streams: Vec<ResultStreamContract>,
    pub required_permissions: Vec<String>,
    pub transaction_policy: TransactionPolicy,
    pub compatibility_policy: CompatibilityPolicy,
    pub result_metadata_policy: ResultMetadataPolicy,
    pub error_policy: ProcedureErrorPolicy,
    pub multi_result_policy: MultiResultPolicy,
}

impl ProcedureContract {
    pub fn as_ref(&self) -> ProcedureContractRef {
        ProcedureContractRef {
            procedure_id: self.procedure_id,
            contract_hash: self.contract_hash,
            catalog_version: self.object.catalog_version,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.object.validate_for_definition(ObjectKind::Procedure)?;
        self.as_ref().validate()?;
        validate_columns_allow_empty(&self.inputs)?;
        if self.stats_version.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure stats version must not be zero",
            ));
        }
        self.protocol_layout.validate()?;
        self.error_policy.validate()?;

        if self.required_permissions.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                "procedure contract must declare required permissions",
            ));
        }
        let mut permissions = std::collections::BTreeSet::new();
        for permission in &self.required_permissions {
            if permission.trim().is_empty() {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Security,
                    "procedure required permissions must not be empty",
                ));
            }
            if !permissions.insert(permission.as_str()) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Security,
                    "procedure required permissions must be unique",
                ));
            }
        }

        let mut structured_input_names = std::collections::BTreeSet::new();
        for structured_input in &self.structured_inputs {
            if !structured_input_names.insert(structured_input) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure structured input names must be unique",
                ));
            }
        }

        let mut result_stream_names = std::collections::BTreeSet::new();
        let mut result_stream_ids = std::collections::BTreeSet::new();
        for stream in &self.result_streams {
            stream.validate()?;
            if !result_stream_ids.insert(stream.stream_id) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure result stream ids must be unique",
                ));
            }
            if !result_stream_names.insert(stream.name.as_str()) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure result stream names must be unique",
                ));
            }
        }

        if self.multi_result_policy == MultiResultPolicy::SingleResultOnly
            && self.result_streams.len() > 1
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure multi-result policy allows only one result stream",
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_types::{ScalarType, TypeDescriptor};

    fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
        ColumnDescriptor {
            name: name.to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal,
        }
    }

    #[test]
    fn result_stream_cardinality_tags_are_stable() {
        let cases = [
            (ResultStreamCardinality::One, 0, true, 1, Some(1)),
            (ResultStreamCardinality::OptionalOne, 1, false, 0, Some(1)),
            (ResultStreamCardinality::Many, 2, false, 0, None),
            (ResultStreamCardinality::NonEmptyMany, 3, true, 1, None),
        ];

        for (cardinality, tag, legacy_required, min_rows, max_rows) in cases {
            assert_eq!(cardinality.stable_tag(), tag);
            assert_eq!(
                ResultStreamCardinality::from_stable_tag(tag).unwrap(),
                cardinality
            );
            assert_eq!(
                cardinality.legacy_row_count_exact_required(),
                legacy_required
            );
            assert_eq!(cardinality.min_row_count(), min_rows);
            assert_eq!(cardinality.intrinsic_max_row_count(), max_rows);
        }

        assert!(ResultStreamCardinality::from_stable_tag(4).is_err());
    }

    #[test]
    fn result_stream_contract_rejects_legacy_cardinality_drift() {
        let stream = ResultStreamContract {
            stream_id: 1,
            name: "Rows".to_string(),
            columns: vec![column("ProductId", 0)],
            cardinality: ResultStreamCardinality::One,
            row_count_exact_required: false,
        };

        assert_eq!(
            stream.validate().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
    }
}
