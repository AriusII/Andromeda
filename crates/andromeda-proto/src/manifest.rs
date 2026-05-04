use andromeda_core::{
    digest::Sha256, AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion,
    ColumnDescriptor, ContractHash, ProcedureId,
};

/// Domain separation tag for the canonical V0 client manifest digest.
///
/// Bumping this tag (or any framing constant in [`ProcedureManifest::manifest_hash`])
/// is a wire break: every field that participates in the digest is length-prefixed
/// and tagged so that re-orderings, truncations, or appended fields yield distinct
/// hashes. Any future field added to the manifest **must** be folded into this
/// digest with its own tag and length prefix, otherwise generated clients can be
/// forked silently.
const MANIFEST_DIGEST_DOMAIN: &[u8] = b"andromeda-procedure-manifest-v0";

/// Versioning bytes for the policy bundle backing a procedure manifest.
///
/// `ManifestPolicyVersion` is intentionally distinct from `ContractHash` so the
/// type system enforces that the policy version cannot be confused with the
/// contract / structured-object / descriptor hashes. The on-the-wire shape is a
/// 32-byte digest, matching `andromeda-catalog::PolicyVersion` so values can be
/// transcribed without re-hashing.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ManifestPolicyVersion([u8; ContractHash::LEN]);

impl ManifestPolicyVersion {
    pub const LEN: usize = ContractHash::LEN;

    pub const fn new(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub const fn zero() -> Self {
        Self([0; Self::LEN])
    }

    pub const fn test_vector(byte: u8) -> Self {
        Self([byte; Self::LEN])
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }
}

impl Default for ManifestPolicyVersion {
    fn default() -> Self {
        Self::zero()
    }
}

impl core::fmt::Debug for ManifestPolicyVersion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ManifestPolicyVersion(")?;
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        write!(f, ")")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolLayout {
    pub descriptor_set_hash: ContractHash,
    pub frame_envelope_hash: ContractHash,
}

impl ProtocolLayout {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.descriptor_set_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "protocol layout descriptor set hash must not be zero",
            ));
        }

        if self.frame_envelope_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "protocol layout frame envelope hash must not be zero",
            ));
        }

        if self.descriptor_set_hash == self.frame_envelope_hash {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "protocol layout descriptor set hash must be distinct from frame envelope hash",
            ));
        }

        Ok(())
    }
}

/// Stable identifier for a permission required to invoke a procedure.
///
/// The manifest stores permissions as canonical string ids rather than a Rust
/// enum so client source generators can map them onto each language's permission
/// catalogue without re-coupling the protocol crate to the security crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredPermission {
    /// Canonical permission id, e.g. `"andromeda.execute_procedure"`. Must be
    /// non-empty, lower-case, dot-separated.
    pub id: String,
    /// Permission family id (e.g. `"application"`, `"security"`). Must be
    /// non-empty.
    pub family: String,
}

impl RequiredPermission {
    pub fn new(id: impl Into<String>, family: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            family: family.into(),
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.id.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "required permission id must not be empty",
            ));
        }

        if self.id != self.id.to_ascii_lowercase() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "required permission id must be lower-case",
            ));
        }

        if self.family.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "required permission family must not be empty",
            ));
        }

        Ok(())
    }
}

/// Source-generator-ready V0 client manifest for a single procedure.
///
/// Carries every fact a generated client needs to bind to a procedure without
/// any runtime negotiation: identity, contract hash, protocol layout
/// descriptors, declared result shapes/cardinality/row bounds, required
/// permissions and policy version. The manifest itself is digestable
/// (`manifest_hash`) so generators can pin the exact manifest revision they
/// emitted code for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureManifest {
    pub procedure_id: ProcedureId,
    pub procedure_name: String,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
    pub policy_version: ManifestPolicyVersion,
    pub protocol_layout: ProtocolLayout,
    pub result_streams: Vec<ResultStreamDescriptor>,
    pub required_permissions: Vec<RequiredPermission>,
}

impl ProcedureManifest {
    /// Validate field-level invariants. This is the gate every producer must
    /// pass before persisting / publishing a manifest. It does **not** require
    /// permissions or policy to be present — those are tightened by
    /// [`Self::ensure_source_generator_ready`].
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.procedure_name.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure manifest name must not be empty",
            ));
        }

        if self.contract_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure manifest contract hash must not be zero",
            ));
        }

        self.protocol_layout.validate()?;

        if self.contract_hash == self.protocol_layout.descriptor_set_hash
            || self.contract_hash == self.protocol_layout.frame_envelope_hash
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure contract hash must be distinct from protocol layout descriptor hashes",
            ));
        }

        let mut seen_streams = std::collections::BTreeSet::new();
        for stream in &self.result_streams {
            stream.validate()?;
            if !seen_streams.insert(stream.stream_name.clone()) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure manifest result stream names must be unique",
                ));
            }
        }

        let mut seen_perms = std::collections::BTreeSet::new();
        for perm in &self.required_permissions {
            perm.validate()?;
            if !seen_perms.insert(perm.id.clone()) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure manifest required permissions must be unique",
                ));
            }
        }

        Ok(())
    }

    /// Stronger gate: the manifest must be ready to drive a source generator.
    /// In addition to [`Self::validate`], every field source-generators rely
    /// on must be explicitly populated:
    ///
    /// - non-empty `required_permissions` (procedure-only execution requires
    ///   at least the procedure-execute permission to be declared),
    /// - non-zero `policy_version` (no implicit default policy bundle),
    /// - non-zero `catalog_version`,
    /// - non-zero `procedure_id`,
    /// - protocol layout descriptor and frame envelope hashes are present
    ///   and distinct (already enforced by [`ProtocolLayout::validate`]).
    pub fn ensure_source_generator_ready(&self) -> AndromedaResult<()> {
        self.validate()?;

        if self.procedure_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "source-generator-ready manifest requires a non-zero procedure id",
            ));
        }

        if self.catalog_version.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "source-generator-ready manifest requires a non-zero catalog version",
            ));
        }

        if self.policy_version.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "source-generator-ready manifest requires an explicit policy version",
            ));
        }

        if self.required_permissions.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "source-generator-ready manifest requires at least one required permission",
            ));
        }

        Ok(())
    }

    /// Deterministic SHA-256 digest of the manifest in canonical encoding.
    ///
    /// The digest is field-tagged and length-prefixed so any reorder /
    /// truncation / appended field changes the hash. Generators pin this
    /// digest into the artifacts they emit to detect manifest drift.
    pub fn manifest_hash(&self) -> ContractHash {
        let mut hasher = Sha256::new();
        write_tagged(&mut hasher, b"domain", MANIFEST_DIGEST_DOMAIN);
        write_tagged(
            &mut hasher,
            b"procedure_id",
            &self.procedure_id.get().to_be_bytes(),
        );
        write_tagged(
            &mut hasher,
            b"procedure_name",
            self.procedure_name.as_bytes(),
        );
        write_tagged(
            &mut hasher,
            b"contract_hash",
            &self.contract_hash.as_bytes(),
        );
        write_tagged(
            &mut hasher,
            b"catalog_version",
            &self.catalog_version.get().to_be_bytes(),
        );
        write_tagged(
            &mut hasher,
            b"policy_version",
            &self.policy_version.as_bytes(),
        );
        write_tagged(
            &mut hasher,
            b"protocol_layout.descriptor_set_hash",
            &self.protocol_layout.descriptor_set_hash.as_bytes(),
        );
        write_tagged(
            &mut hasher,
            b"protocol_layout.frame_envelope_hash",
            &self.protocol_layout.frame_envelope_hash.as_bytes(),
        );

        hasher.update(b"result_streams:");
        hasher.update(&(self.result_streams.len() as u64).to_be_bytes());
        for stream in &self.result_streams {
            stream.absorb_into(&mut hasher);
        }

        hasher.update(b"required_permissions:");
        hasher.update(&(self.required_permissions.len() as u64).to_be_bytes());
        for perm in &self.required_permissions {
            write_tagged(&mut hasher, b"perm.id", perm.id.as_bytes());
            write_tagged(&mut hasher, b"perm.family", perm.family.as_bytes());
        }

        ContractHash::new(hasher.finalize())
    }
}

fn write_tagged(hasher: &mut Sha256, tag: &[u8], value: &[u8]) {
    hasher.update(tag);
    hasher.update(b":");
    hasher.update(&(value.len() as u64).to_be_bytes());
    hasher.update(value);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultStreamDescriptor {
    pub stream_name: String,
    pub columns: Vec<ColumnDescriptor>,
    pub cardinality: ResultCardinality,
    pub row_count_requirement: RowCountRequirement,
    pub row_count_exact: Option<u64>,
    /// Optional inclusive upper bound on the number of rows the stream may
    /// emit. Used to declare a *bounded* `ZeroOrMore` / `OneOrMore` result
    /// without inventing a streaming engine. When present, must respect the
    /// declared cardinality (intrinsic max of 1 for `ZeroOrOne`/`ExactlyOne`,
    /// minimum of 1 for `OneOrMore`) and must be `>= row_count_exact` when
    /// both are declared.
    pub row_count_max: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultCardinality {
    ZeroOrMore,
    ZeroOrOne,
    OneOrMore,
    ExactlyOne,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowCountRequirement {
    UnknownAllowed,
    ExactIfKnown,
    ExactRequired,
}

impl ResultCardinality {
    /// Lower bound on the number of rows a stream of this cardinality may
    /// emit.
    pub const fn min_row_count(self) -> u64 {
        match self {
            Self::ExactlyOne | Self::OneOrMore => 1,
            Self::ZeroOrOne | Self::ZeroOrMore => 0,
        }
    }

    /// Intrinsic upper bound carried by the cardinality kind itself.
    /// `ExactlyOne` and `ZeroOrOne` are intrinsically bounded at 1.
    pub const fn intrinsic_max_row_count(self) -> Option<u64> {
        match self {
            Self::ExactlyOne | Self::ZeroOrOne => Some(1),
            Self::OneOrMore | Self::ZeroOrMore => None,
        }
    }

    /// True when an explicit upper bound is consistent with this cardinality
    /// (i.e. it neither contradicts the intrinsic max nor the minimum row
    /// count).
    pub const fn permits_row_count_max(self, row_count_max: u64) -> bool {
        if row_count_max < self.min_row_count() {
            return false;
        }
        match self.intrinsic_max_row_count() {
            Some(intrinsic) => row_count_max <= intrinsic,
            None => true,
        }
    }
}

impl ResultStreamDescriptor {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.stream_name.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream descriptor name must not be empty",
            ));
        }

        for column in &self.columns {
            column.validate()?;
        }

        if self.row_count_requirement == RowCountRequirement::ExactRequired
            && self.row_count_exact.is_none()
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream requires an exact row count",
            ));
        }

        if let Some(row_count_exact) = self.row_count_exact {
            match self.cardinality {
                ResultCardinality::ZeroOrMore => {}
                ResultCardinality::ZeroOrOne if row_count_exact <= 1 => {}
                ResultCardinality::OneOrMore if row_count_exact >= 1 => {}
                ResultCardinality::ExactlyOne if row_count_exact == 1 => {}
                _ => {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Contract,
                        "exact row count violates result stream cardinality",
                    ));
                }
            }
        }

        if let Some(row_count_max) = self.row_count_max {
            if !self.cardinality.permits_row_count_max(row_count_max) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "result stream row_count_max violates cardinality bounds",
                ));
            }
            if let Some(row_count_exact) = self.row_count_exact {
                if row_count_exact > row_count_max {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Contract,
                        "result stream row_count_exact exceeds declared row_count_max",
                    ));
                }
            }
        }

        Ok(())
    }

    /// Fold the descriptor into a [`Sha256`] hasher in a length-prefixed,
    /// field-tagged canonical form. Used by [`ProcedureManifest::manifest_hash`]
    /// to keep the manifest digest deterministic across builds.
    fn absorb_into(&self, hasher: &mut Sha256) {
        write_tagged(hasher, b"stream.name", self.stream_name.as_bytes());
        hasher.update(b"stream.columns:");
        hasher.update(&(self.columns.len() as u64).to_be_bytes());
        for column in &self.columns {
            write_tagged(hasher, b"column.name", column.name.as_bytes());
            hasher.update(b"column.ordinal:");
            hasher.update(&column.ordinal.to_be_bytes());
            // Debug formatting on TypeDescriptor is exhaustive over the enum
            // variants and stable for our purposes; any change to the
            // descriptor shape is a deliberate wire break and is allowed to
            // change the manifest digest.
            let formatted = format!("{:?}", column.data_type);
            write_tagged(hasher, b"column.type", formatted.as_bytes());
        }
        hasher.update(b"stream.cardinality:");
        hasher.update(&[self.cardinality as u8]);
        hasher.update(b"stream.row_count_requirement:");
        hasher.update(&[self.row_count_requirement as u8]);
        hasher.update(b"stream.row_count_exact:");
        match self.row_count_exact {
            Some(v) => {
                hasher.update(&[1]);
                hasher.update(&v.to_be_bytes());
            }
            None => hasher.update(&[0]),
        }
        hasher.update(b"stream.row_count_max:");
        match self.row_count_max {
            Some(v) => {
                hasher.update(&[1]);
                hasher.update(&v.to_be_bytes());
            }
            None => hasher.update(&[0]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::{ScalarType, TypeDescriptor};

    #[test]
    fn result_descriptor_validates_columns() {
        let descriptor = ResultStreamDescriptor {
            stream_name: "Reservation".to_string(),
            columns: vec![ColumnDescriptor {
                name: "ProductId".to_string(),
                data_type: TypeDescriptor::required(ScalarType::I64),
                ordinal: 0,
            }],
            cardinality: ResultCardinality::ExactlyOne,
            row_count_requirement: RowCountRequirement::ExactRequired,
            row_count_exact: Some(1),
            row_count_max: Some(1),
        };

        assert!(descriptor.validate().is_ok());
    }

    #[test]
    fn result_descriptor_rejects_inconsistent_row_count_max() {
        fn col() -> ColumnDescriptor {
            ColumnDescriptor {
                name: "x".to_string(),
                data_type: TypeDescriptor::required(ScalarType::I64),
                ordinal: 0,
            }
        }

        // ExactlyOne with max > 1 is contradictory.
        let bad = ResultStreamDescriptor {
            stream_name: "S".to_string(),
            columns: vec![col()],
            cardinality: ResultCardinality::ExactlyOne,
            row_count_requirement: RowCountRequirement::ExactRequired,
            row_count_exact: Some(1),
            row_count_max: Some(2),
        };
        assert!(bad.validate().is_err());

        // OneOrMore with max 0 is contradictory (min row count is 1).
        let bad_min = ResultStreamDescriptor {
            stream_name: "S".to_string(),
            columns: vec![col()],
            cardinality: ResultCardinality::OneOrMore,
            row_count_requirement: RowCountRequirement::ExactIfKnown,
            row_count_exact: None,
            row_count_max: Some(0),
        };
        assert!(bad_min.validate().is_err());

        // exact > max is contradictory.
        let bad_dom = ResultStreamDescriptor {
            stream_name: "S".to_string(),
            columns: vec![col()],
            cardinality: ResultCardinality::ZeroOrMore,
            row_count_requirement: RowCountRequirement::ExactIfKnown,
            row_count_exact: Some(5),
            row_count_max: Some(3),
        };
        assert!(bad_dom.validate().is_err());

        // Bounded ZeroOrMore with consistent declarations validates.
        let ok = ResultStreamDescriptor {
            stream_name: "S".to_string(),
            columns: vec![col()],
            cardinality: ResultCardinality::ZeroOrMore,
            row_count_requirement: RowCountRequirement::ExactIfKnown,
            row_count_exact: None,
            row_count_max: Some(64),
        };
        assert!(ok.validate().is_ok());
    }

    fn sample_column(name: &str, ordinal: u32) -> ColumnDescriptor {
        ColumnDescriptor {
            name: name.to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal,
        }
    }

    fn sample_stream() -> ResultStreamDescriptor {
        ResultStreamDescriptor {
            stream_name: "Reservation".to_string(),
            columns: vec![sample_column("ProductId", 0)],
            cardinality: ResultCardinality::ExactlyOne,
            row_count_requirement: RowCountRequirement::ExactRequired,
            row_count_exact: Some(1),
            row_count_max: Some(1),
        }
    }

    fn sample_manifest() -> ProcedureManifest {
        ProcedureManifest {
            procedure_id: ProcedureId::new(42),
            procedure_name: "Inventory.ReserveStock".to_string(),
            contract_hash: ContractHash::test_vector(0x11),
            catalog_version: CatalogVersion::new(7),
            policy_version: ManifestPolicyVersion::test_vector(0x22),
            protocol_layout: ProtocolLayout {
                descriptor_set_hash: ContractHash::test_vector(0x33),
                frame_envelope_hash: ContractHash::test_vector(0x44),
            },
            result_streams: vec![sample_stream()],
            required_permissions: vec![RequiredPermission::new(
                "andromeda.execute_procedure",
                "application",
            )],
        }
    }

    #[test]
    fn manifest_hash_is_deterministic_and_field_sensitive() {
        let manifest = sample_manifest();
        assert!(!manifest.manifest_hash().is_zero());
        assert_eq!(manifest.manifest_hash(), manifest.clone().manifest_hash());

        // Mutating any participating field flips the hash.
        let mut renamed = manifest.clone();
        renamed.procedure_name.push_str("_v2");
        assert_ne!(manifest.manifest_hash(), renamed.manifest_hash());

        let mut bumped_policy = manifest.clone();
        bumped_policy.policy_version = ManifestPolicyVersion::test_vector(0x55);
        assert_ne!(manifest.manifest_hash(), bumped_policy.manifest_hash());

        let mut extra_perm = manifest.clone();
        extra_perm
            .required_permissions
            .push(RequiredPermission::new(
                "andromeda.read_contract",
                "application",
            ));
        assert_ne!(manifest.manifest_hash(), extra_perm.manifest_hash());

        let mut wider_layout = manifest.clone();
        wider_layout.protocol_layout.descriptor_set_hash = ContractHash::test_vector(0x77);
        assert_ne!(manifest.manifest_hash(), wider_layout.manifest_hash());
    }

    #[test]
    fn manifest_validate_rejects_missing_or_invalid_fields() {
        let base = sample_manifest();
        assert!(base.validate().is_ok());

        let blank_name = ProcedureManifest {
            procedure_name: "   ".to_string(),
            ..base.clone()
        };
        assert_eq!(
            blank_name.validate().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );

        let zero_contract = ProcedureManifest {
            contract_hash: ContractHash::zero(),
            ..base.clone()
        };
        assert_eq!(
            zero_contract.validate().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );

        let dup_streams = ProcedureManifest {
            result_streams: vec![sample_stream(), sample_stream()],
            ..base.clone()
        };
        assert_eq!(
            dup_streams.validate().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );

        let dup_perms = ProcedureManifest {
            required_permissions: vec![
                RequiredPermission::new("andromeda.execute_procedure", "application"),
                RequiredPermission::new("andromeda.execute_procedure", "application"),
            ],
            ..base.clone()
        };
        assert_eq!(
            dup_perms.validate().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );

        let upper_perm = ProcedureManifest {
            required_permissions: vec![RequiredPermission::new(
                "Andromeda.Execute_Procedure",
                "application",
            )],
            ..base
        };
        assert_eq!(
            upper_perm.validate().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
    }

    #[test]
    fn protocol_layout_separates_descriptor_and_frame_hashes() {
        let collide = ProtocolLayout {
            descriptor_set_hash: ContractHash::test_vector(0xAA),
            frame_envelope_hash: ContractHash::test_vector(0xAA),
        };
        assert_eq!(
            collide.validate().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );

        let zero = ProtocolLayout {
            descriptor_set_hash: ContractHash::zero(),
            frame_envelope_hash: ContractHash::test_vector(0xAA),
        };
        assert_eq!(
            zero.validate().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );

        let collide_with_contract = ProcedureManifest {
            contract_hash: ContractHash::test_vector(0x33),
            ..sample_manifest()
        };
        // contract_hash matches descriptor_set_hash in the sample manifest.
        assert_eq!(
            collide_with_contract.validate().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
    }

    #[test]
    fn ensure_source_generator_ready_demands_explicit_metadata() {
        let manifest = sample_manifest();
        assert!(manifest.ensure_source_generator_ready().is_ok());

        let zero_proc = ProcedureManifest {
            procedure_id: ProcedureId::new(0),
            ..manifest.clone()
        };
        assert_eq!(
            zero_proc
                .ensure_source_generator_ready()
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Contract
        );

        let zero_catalog = ProcedureManifest {
            catalog_version: CatalogVersion::new(0),
            ..manifest.clone()
        };
        assert_eq!(
            zero_catalog
                .ensure_source_generator_ready()
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Contract
        );

        let zero_policy = ProcedureManifest {
            policy_version: ManifestPolicyVersion::zero(),
            ..manifest.clone()
        };
        assert_eq!(
            zero_policy
                .ensure_source_generator_ready()
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Contract
        );

        let no_perms = ProcedureManifest {
            required_permissions: Vec::new(),
            ..manifest
        };
        assert_eq!(
            no_perms.ensure_source_generator_ready().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
    }

    #[test]
    fn required_permission_validation_rejects_blank_or_uppercase_ids() {
        assert!(
            RequiredPermission::new("andromeda.execute_procedure", "application")
                .validate()
                .is_ok()
        );
        assert!(RequiredPermission::new("", "application")
            .validate()
            .is_err());
        assert!(RequiredPermission::new("andromeda.execute_procedure", "")
            .validate()
            .is_err());
        assert!(RequiredPermission::new("Andromeda.X", "application")
            .validate()
            .is_err());
    }
}
