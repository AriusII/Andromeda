//! Core contract types: enums, structs, and their basic constructors and validation.

use std::collections::BTreeSet;

use andromeda_contract_compat::{
    CONTRACT_COMPAT_COMPATIBLE_ADDITIVE, CONTRACT_COMPAT_COMPATIBLE_METADATA_ONLY,
    CONTRACT_COMPAT_INCOMPATIBLE_PERMISSION, CONTRACT_COMPAT_INCOMPATIBLE_SHAPE,
    CONTRACT_COMPAT_REVIEW_REQUIRED,
};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, ProcedureId,
};

use crate::names::QualifiedName;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Database,
    Namespace,
    Table,
    Map,
    Enum,
    StructuredObject,
    Procedure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogObjectRef {
    pub object_id: CatalogObjectId,
    pub name: QualifiedName,
    pub kind: ObjectKind,
    pub catalog_version: CatalogVersion,
}

impl CatalogObjectRef {
    pub fn validate_for_definition(&self, expected_kind: ObjectKind) -> AndromedaResult<()> {
        if self.object_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog object id must not be zero",
            ));
        }

        if self.catalog_version.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog object version must not be zero",
            ));
        }

        if self.kind != expected_kind {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog object kind must match its definition",
            ));
        }

        Ok(())
    }
}

fn validate_columns(columns: &[ColumnDescriptor]) -> AndromedaResult<()> {
    validate_columns_with_min(columns, true)
}

fn validate_columns_allow_empty(columns: &[ColumnDescriptor]) -> AndromedaResult<()> {
    validate_columns_with_min(columns, false)
}

fn validate_columns_with_min(
    columns: &[ColumnDescriptor],
    require_non_empty: bool,
) -> AndromedaResult<()> {
    if require_non_empty && columns.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "column list must not be empty",
        ));
    }

    let mut column_names = BTreeSet::new();
    for (expected_ordinal, column) in columns.iter().enumerate() {
        column.validate()?;
        if !column_names.insert(column.name.as_str()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "column names must be unique",
            ));
        }
        if column.ordinal != expected_ordinal as u32 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "column ordinals must be dense and zero-based",
            ));
        }
    }

    Ok(())
}

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

/// Typed compatibility decision produced by the contract compatibility checker.
///
/// Every operation in a `DefinitionBatch` carries exactly one
/// `CompatibilityDecision` (per `SPEC_DEFINITION_BATCH_V0.md` §Invariants).
/// Use [`CompatibilityDecision::taxonomy_id`] to obtain the stable
/// `andromeda.contract.compat.v0` identifier for audit and trace evidence.
///
/// # Decision priority (when multiple signals are detected)
///
/// `Rejected` > `SecurityImpact` > `Breaking` > `Additive`
///
/// # Note on `Deprecated`
///
/// `Deprecated` is a lifecycle-driven decision that is **not** produced by
/// [`crate::diagnose_procedure_contract_compatibility`].  It is reserved for
/// `DefinitionOperation::Deprecate` — a DefinitionBatch operation not yet
/// implemented.
///
/// TODO: wire `Deprecated` when `DefinitionOperation::Deprecate` is added
/// (`SPEC_DEFINITION_BATCH_V0.md` §Invariants — "lifecycle-driven, attach to
/// receipt only when DefinitionOperation::Deprecate").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatibilityDecision {
    /// The contract change is strictly additive (or identical under
    /// `ExactHash` policy).  This is the only non-advisory acceptable
    /// decision.
    Additive,
    /// The contract change is breaking — callers or downstream systems that
    /// bound the previous contract version will fail.
    Breaking { reasons: Vec<String> },
    /// The contract change crosses a security boundary (e.g. a required
    /// permission was removed or the transaction access mode widens from
    /// `ReadOnly` to `ReadWrite`).
    SecurityImpact { reasons: Vec<String> },
    /// Lifecycle-driven deprecation.  Acceptable but advisory: the procedure
    /// remains callable but its removal is signalled.  Not produced by
    /// `diagnose_procedure_contract_compatibility` — reserved for
    /// `DefinitionOperation::Deprecate`.
    Deprecated,
    /// The operation was rejected outright (invalid identity, non-canonical
    /// hash, non-advancing `CatalogVersion`, etc.).
    Rejected { reasons: Vec<String> },
}

impl CompatibilityDecision {
    /// Returns `true` when the decision allows the operation to proceed
    /// without a policy override.
    ///
    /// - `Additive` — unconditionally acceptable.
    /// - `Deprecated` — acceptable but advisory (lifecycle signal only).
    /// - `Breaking`, `SecurityImpact`, `Rejected` — not acceptable; require
    ///   an explicit policy override or must be rejected.
    pub fn is_acceptable(&self) -> bool {
        matches!(self, Self::Additive | Self::Deprecated)
    }

    /// Returns the stable taxonomy identifier from
    /// `andromeda.contract.compat.v0` that corresponds to this decision.
    ///
    /// The returned string is one of the `CONTRACT_COMPAT_*` constants owned
    /// by `andromeda-contract-compat`.  It is suitable for audit records,
    /// decision traces, and operator-facing evidence.
    pub fn taxonomy_id(&self) -> &'static str {
        match self {
            Self::Additive => CONTRACT_COMPAT_COMPATIBLE_ADDITIVE,
            Self::Breaking { .. } => CONTRACT_COMPAT_INCOMPATIBLE_SHAPE,
            Self::SecurityImpact { .. } => CONTRACT_COMPAT_INCOMPATIBLE_PERMISSION,
            Self::Deprecated => CONTRACT_COMPAT_COMPATIBLE_METADATA_ONLY,
            Self::Rejected { .. } => CONTRACT_COMPAT_REVIEW_REQUIRED,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractCompatibilityDiagnostic {
    /// `true` when `decision.is_acceptable()`.
    ///
    /// Kept for backward compatibility with existing callers.  New code
    /// should pattern-match on [`Self::decision`] directly.
    pub compatible: bool,
    /// Mirrors the `reasons` carried by the decision variant; empty for
    /// `Additive` and `Deprecated`.
    ///
    /// Kept for backward compatibility.  Prefer inspecting
    /// `decision` in new code to avoid interpreting the free-form strings.
    pub messages: Vec<String>,
    /// The typed compatibility decision.
    pub decision: CompatibilityDecision,
}

impl ContractCompatibilityDiagnostic {
    /// Construct an `Additive` diagnostic (backward-compatible alias).
    pub fn compatible() -> Self {
        Self::with_decision(CompatibilityDecision::Additive)
    }

    /// Construct a `Breaking` diagnostic from a free-form message list
    /// (backward-compatible alias).
    ///
    /// Prefer [`Self::with_decision`] in new code.
    pub fn incompatible(messages: Vec<String>) -> Self {
        Self::with_decision(CompatibilityDecision::Breaking { reasons: messages })
    }

    /// Construct a diagnostic from a fully-typed [`CompatibilityDecision`].
    ///
    /// The `compatible` and `messages` fields are derived from the decision
    /// so that existing callers that read those fields continue to work
    /// without modification.
    pub fn with_decision(decision: CompatibilityDecision) -> Self {
        let compatible = decision.is_acceptable();
        let messages = match &decision {
            CompatibilityDecision::Additive | CompatibilityDecision::Deprecated => Vec::new(),
            CompatibilityDecision::Breaking { reasons }
            | CompatibilityDecision::SecurityImpact { reasons }
            | CompatibilityDecision::Rejected { reasons } => reasons.clone(),
        };
        Self {
            compatible,
            messages,
            decision,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
        let mut codes = BTreeSet::new();
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
        let mut permissions = BTreeSet::new();
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

        let mut structured_input_names = BTreeSet::new();
        for structured_input in &self.structured_inputs {
            if !structured_input_names.insert(structured_input) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure structured input names must be unique",
                ));
            }
        }

        let mut result_stream_names = BTreeSet::new();
        let mut result_stream_ids = BTreeSet::new();
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

/// Opaque binary payload carrying the encoded input parameters for a procedure
/// dispatch request.
///
/// An empty payload is valid for procedures that accept no inputs. Callers are
/// responsible for encoding parameters according to the procedure contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureDispatchPayload(Vec<u8>);

impl ProcedureDispatchPayload {
    /// Wraps raw bytes as a dispatch payload.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Creates an empty payload (valid for zero-input procedures).
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    /// Returns `true` if the payload contains no bytes.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the payload as a byte slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Consumes the payload and returns the underlying bytes.
    pub fn into_vec(self) -> Vec<u8> {
        self.0
    }
}

/// Maximum wall-clock duration allowed for a procedure dispatch to complete.
///
/// The dispatch boundary validates that the deadline is non-zero before
/// forwarding the request to the execution layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProcedureDeadline(std::time::Duration);

impl ProcedureDeadline {
    /// Wraps a `Duration` as the dispatch deadline.
    ///
    /// The value is not validated here; callers must ensure the deadline is
    /// non-zero or invoke `ProcedureDispatchRequest::validate()`.
    pub fn new(duration: std::time::Duration) -> Self {
        Self(duration)
    }

    /// Returns the underlying duration.
    pub fn as_duration(self) -> std::time::Duration {
        self.0
    }

    /// Returns `true` if the deadline is zero (invalid for dispatch).
    pub fn is_zero(self) -> bool {
        self.0.is_zero()
    }
}

/// A 32-byte idempotency key that identifies a specific dispatch attempt.
///
/// When provided, the execution layer uses the key to detect and suppress
/// duplicate invocations caused by client retries. Identical keys for the
/// same `ProcedureId` are treated as the same logical request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IdempotencyKey([u8; 32]);

impl IdempotencyKey {
    /// Wraps 32 raw bytes as an idempotency key.
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the underlying 32-byte key.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
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
