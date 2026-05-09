use andromeda_digest::Sha256;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

use super::{ManifestPolicyVersion, ProtocolLayout, RequiredPermission, ResultStreamDescriptor};

/// Domain separation tag for the canonical V0 client manifest digest.
///
/// Bumping this tag (or any framing constant in [`ProcedureManifest::manifest_hash`])
/// is a wire break: every field that participates in the digest is length-prefixed
/// and tagged so that re-orderings, truncations, or appended fields yield distinct
/// hashes. Any future field added to the manifest **must** be folded into this
/// digest with its own tag and length prefix, otherwise generated clients can be
/// forked silently.
const MANIFEST_DIGEST_DOMAIN: &[u8] = b"andromeda-procedure-manifest-v0";

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
    pub stats_version: u64,
    pub policy_version: ManifestPolicyVersion,
    pub protocol_layout: ProtocolLayout,
    pub result_streams: Vec<ResultStreamDescriptor>,
    pub required_permissions: Vec<RequiredPermission>,
}

/// Contract projection of the catalog `ProcedureContractBinding`.
///
/// This type mirrors the binding identities without depending on
/// `andromeda-catalog`, keeping procedure manifests as a runtime-free contract
/// model instead of an internal catalog domain model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcedureManifestBinding {
    pub procedure_id: ProcedureId,
    pub catalog_version: CatalogVersion,
    pub contract_hash: ContractHash,
    pub stats_version: u64,
    pub policy_version: ManifestPolicyVersion,
}

impl ProcedureManifestBinding {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.procedure_id.get() == 0 {
            return Err(contract_error(
                "procedure manifest binding id must not be zero",
            ));
        }

        if self.catalog_version.get() == 0 {
            return Err(contract_error(
                "procedure manifest binding catalog version must not be zero",
            ));
        }

        if self.contract_hash.is_zero() {
            return Err(contract_error(
                "procedure manifest binding contract hash must not be zero",
            ));
        }

        if self.stats_version == 0 {
            return Err(contract_error(
                "procedure manifest binding stats version must not be zero",
            ));
        }

        if self.policy_version.is_zero() {
            return Err(contract_error(
                "procedure manifest binding policy version must not be zero",
            ));
        }

        Ok(())
    }
}

impl ProcedureManifest {
    /// Validate field-level invariants. This is the gate every producer must
    /// pass before persisting / publishing a manifest. It does **not** require
    /// permissions or policy to be present — those are tightened by
    /// [`Self::ensure_source_generator_ready`].
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.procedure_name.trim().is_empty() {
            return Err(contract_error("procedure manifest name must not be empty"));
        }

        if self.contract_hash.is_zero() {
            return Err(contract_error(
                "procedure manifest contract hash must not be zero",
            ));
        }

        self.protocol_layout.validate()?;

        if self.contract_hash == self.protocol_layout.descriptor_set_hash
            || self.contract_hash == self.protocol_layout.frame_envelope_hash
        {
            return Err(contract_error(
                "procedure contract hash must be distinct from protocol layout descriptor hashes",
            ));
        }

        let mut seen_streams = std::collections::BTreeSet::new();
        for stream in &self.result_streams {
            stream.validate()?;
            if !seen_streams.insert(stream.stream_name.clone()) {
                return Err(contract_error(
                    "procedure manifest result stream names must be unique",
                ));
            }
        }

        let mut seen_perms = std::collections::BTreeSet::new();
        for perm in &self.required_permissions {
            perm.validate()?;
            if !seen_perms.insert(perm.id.clone()) {
                return Err(contract_error(
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
    /// - non-zero `procedure_id`,
    /// - non-zero `catalog_version`,
    /// - non-zero `stats_version`,
    /// - non-zero `policy_version` (no implicit default policy bundle),
    /// - protocol layout descriptor and frame envelope hashes are present
    ///   and distinct (already enforced by [`ProtocolLayout::validate`]).
    pub fn ensure_source_generator_ready(&self) -> AndromedaResult<()> {
        self.validate()?;
        self.binding().validate()?;

        if self.procedure_id.get() == 0 {
            return Err(contract_error(
                "source-generator-ready manifest requires a non-zero procedure id",
            ));
        }

        if self.catalog_version.get() == 0 {
            return Err(contract_error(
                "source-generator-ready manifest requires a non-zero catalog version",
            ));
        }

        if self.policy_version.is_zero() {
            return Err(contract_error(
                "source-generator-ready manifest requires an explicit policy version",
            ));
        }

        if self.required_permissions.is_empty() {
            return Err(contract_error(
                "source-generator-ready manifest requires at least one required permission",
            ));
        }

        Ok(())
    }

    /// Return the full binding identities carried by the manifest.
    pub const fn binding(&self) -> ProcedureManifestBinding {
        ProcedureManifestBinding {
            procedure_id: self.procedure_id,
            catalog_version: self.catalog_version,
            contract_hash: self.contract_hash,
            stats_version: self.stats_version,
            policy_version: self.policy_version,
        }
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
            b"stats_version",
            &self.stats_version.to_be_bytes(),
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

pub(super) fn write_tagged(hasher: &mut Sha256, tag: &[u8], value: &[u8]) {
    hasher.update(tag);
    hasher.update(b":");
    hasher.update(&(value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn contract_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Contract, message)
}
