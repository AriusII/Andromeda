use andromeda_audit::{Permission, SecurityAuditOutcome, SurfaceScope, UserPrincipalKind};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use sha2::{Digest, Sha256};

use crate::{
    cluster_security::{
        HadrClusterManifestVersion, HadrClusterOperation, HadrClusterSecurityEvidence,
    },
    promotion_boundary::PromotionCommit,
    types::{HadrEpoch, HadrNodeId},
};

mod cursor;

use cursor::DecodeCursor;

const CLUSTER_MANIFEST_MAGIC: &[u8; 16] = b"AND-HADR-CMANV0\0";
const CLUSTER_MANIFEST_CODEC_VERSION_V0: u16 = 1;
const CLUSTER_MANIFEST_CHECKSUM_LEN: usize = 32;
const CLUSTER_MANIFEST_HEADER_LEN: usize = CLUSTER_MANIFEST_MAGIC.len() + 2 + 8 + 32;
const CLUSTER_MANIFEST_MAX_MEMBERS: usize = 64;
const CLUSTER_MANIFEST_MAX_CLUSTER_NAME_BYTES: usize = 128;
const CLUSTER_MANIFEST_FIXED_FIELDS_BYTES: usize = 8 + 8 + 16 + 2 + 8 + 1 + 2 + 2 + 32 + 32 + 32;
const MANIFEST_PUBLICATION_FENCING_HASH_DOMAIN: &[u8] = b"AND-HADR-CMAN-PUBLISH-FENCE-V0\0";
const MANIFEST_PUBLICATION_AUDIT_HASH_DOMAIN: &[u8] = b"AND-HADR-CMAN-PUBLISH-AUDIT-V0\0";
const MANIFEST_PUBLICATION_SECURITY_HASH_DOMAIN: &[u8] = b"AND-HADR-CMAN-PUBLISH-SECURITY-V0\0";
const MANIFEST_PUBLICATION_SUCCESSOR_HASH_DOMAIN: &[u8] = b"AND-HADR-CMAN-PUBLISH-SUCCESSOR-V0\0";
const MANIFEST_PUBLICATION_ROOT_HASH_DOMAIN: &[u8] = b"AND-HADR-CMAN-PUBLISH-ROOT-V0\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HadrClusterId([u8; 16]);

impl HadrClusterId {
    pub const fn new(value: [u8; 16]) -> Self {
        Self(value)
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub fn is_zero(&self) -> bool {
        self.0 == [0; 16]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HadrEvidenceHash([u8; 32]);

impl HadrEvidenceHash {
    pub const fn new(value: [u8; 32]) -> Self {
        Self(value)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn is_zero(&self) -> bool {
        self.0 == [0; 32]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HadrClusterManifestMember {
    pub node_id: HadrNodeId,
}

impl HadrClusterManifestMember {
    pub const fn new(node_id: HadrNodeId) -> Self {
        Self { node_id }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HadrClusterQuorumPolicy {
    Majority,
    Fixed { quorum_size: u16 },
}

impl HadrClusterQuorumPolicy {
    const MAJORITY_TAG: u8 = 1;
    const FIXED_TAG: u8 = 2;

    fn encode(self, encoded: &mut Vec<u8>) {
        match self {
            Self::Majority => {
                encoded.push(Self::MAJORITY_TAG);
                encoded.extend_from_slice(&0u16.to_le_bytes());
            },
            Self::Fixed { quorum_size } => {
                encoded.push(Self::FIXED_TAG);
                encoded.extend_from_slice(&quorum_size.to_le_bytes());
            },
        }
    }

    fn decode(cursor: &mut DecodeCursor<'_>) -> AndromedaResult<Self> {
        let tag = cursor.read_u8()?;
        let quorum_size = cursor.read_u16()?;
        match tag {
            Self::MAJORITY_TAG => Ok(Self::Majority),
            Self::FIXED_TAG => Ok(Self::Fixed { quorum_size }),
            _ => Err(storage_error(
                "cluster manifest quorum policy tag is unsupported",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HadrClusterManifestV0 {
    pub cluster_id: HadrClusterId,
    pub cluster_name: String,
    pub epoch: HadrEpoch,
    pub manifest_version: HadrClusterManifestVersion,
    pub primary_node_id: HadrNodeId,
    pub members: Vec<HadrClusterManifestMember>,
    pub quorum_policy: HadrClusterQuorumPolicy,
    pub fencing_token_hash: HadrEvidenceHash,
    pub promotion_history_hash: HadrEvidenceHash,
    pub root_evidence_hash: HadrEvidenceHash,
}

impl HadrClusterManifestV0 {
    #[allow(
        clippy::too_many_arguments,
        reason = "ClusterManifest v0 fields are explicit by spec."
    )]
    pub fn new(
        cluster_id: HadrClusterId,
        cluster_name: impl Into<String>,
        epoch: HadrEpoch,
        manifest_version: HadrClusterManifestVersion,
        primary_node_id: HadrNodeId,
        members: Vec<HadrClusterManifestMember>,
        quorum_policy: HadrClusterQuorumPolicy,
        fencing_token_hash: HadrEvidenceHash,
        promotion_history_hash: HadrEvidenceHash,
        root_evidence_hash: HadrEvidenceHash,
    ) -> AndromedaResult<Self> {
        let manifest = Self {
            cluster_id,
            cluster_name: cluster_name.into(),
            epoch,
            manifest_version,
            primary_node_id,
            members,
            quorum_policy,
            fencing_token_hash,
            promotion_history_hash,
            root_evidence_hash,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.cluster_id.is_zero() {
            return Err(storage_error(
                "cluster manifest cluster id must not be zero",
            ));
        }
        if self.cluster_name.trim().is_empty() {
            return Err(storage_error(
                "cluster manifest cluster name must not be empty",
            ));
        }
        if self.cluster_name.len() > CLUSTER_MANIFEST_MAX_CLUSTER_NAME_BYTES {
            return Err(storage_error(
                "cluster manifest cluster name exceeds bounded length",
            ));
        }
        if self.epoch.is_zero() {
            return Err(storage_error("cluster manifest epoch must be non-zero"));
        }
        if self.manifest_version == HadrClusterManifestVersion::ZERO {
            return Err(storage_error(
                "cluster manifest version must be greater than zero",
            ));
        }
        if self.primary_node_id.is_zero() {
            return Err(storage_error(
                "cluster manifest primary node id must be non-zero",
            ));
        }
        if self.members.is_empty() {
            return Err(storage_error("cluster manifest members must not be empty"));
        }
        if self.members.len() > CLUSTER_MANIFEST_MAX_MEMBERS {
            return Err(storage_error(
                "cluster manifest members exceed bounded limit",
            ));
        }
        let mut sorted: Vec<HadrNodeId> =
            self.members.iter().map(|member| member.node_id).collect();
        sorted.sort_unstable();
        if sorted.iter().any(|member| member.is_zero()) {
            return Err(storage_error(
                "cluster manifest member node id must be non-zero",
            ));
        }
        for window in sorted.windows(2) {
            if window[0] == window[1] {
                return Err(storage_error(
                    "cluster manifest member node ids must be unique",
                ));
            }
        }
        if !self
            .members
            .iter()
            .any(|member| member.node_id == self.primary_node_id)
        {
            return Err(storage_error(
                "cluster manifest primary node id must reference a member",
            ));
        }
        validate_quorum_policy(self.quorum_policy, self.members.len())?;
        if self.fencing_token_hash.is_zero() {
            return Err(storage_error(
                "cluster manifest fencing token hash must be non-zero",
            ));
        }
        if self.promotion_history_hash.is_zero() {
            return Err(storage_error(
                "cluster manifest promotion history hash must be non-zero",
            ));
        }
        if self.root_evidence_hash.is_zero() {
            return Err(storage_error(
                "cluster manifest root evidence hash must be non-zero",
            ));
        }
        Ok(())
    }

    pub fn ensure_advances(&self, previous: &Self) -> AndromedaResult<()> {
        if self.cluster_id != previous.cluster_id {
            return Err(storage_error(
                "cluster manifest successor must keep cluster id stable",
            ));
        }
        if self.epoch <= previous.epoch {
            return Err(storage_error(
                "cluster manifest successor epoch must strictly advance",
            ));
        }
        if self.manifest_version <= previous.manifest_version {
            return Err(storage_error(
                "cluster manifest successor version must strictly advance",
            ));
        }
        Ok(())
    }

    pub fn encode(&self) -> AndromedaResult<Vec<u8>> {
        self.validate()?;
        let payload = self.encode_payload()?;
        let checksum = Sha256::digest(&payload);
        let mut encoded = Vec::with_capacity(
            CLUSTER_MANIFEST_HEADER_LEN
                .checked_add(payload.len())
                .ok_or_else(|| storage_error("cluster manifest file length overflow"))?,
        );
        encoded.extend_from_slice(CLUSTER_MANIFEST_MAGIC);
        encoded.extend_from_slice(&CLUSTER_MANIFEST_CODEC_VERSION_V0.to_le_bytes());
        let payload_len = u64::try_from(payload.len())
            .map_err(|_| storage_error("cluster manifest payload length exceeds u64"))?;
        encoded.extend_from_slice(&payload_len.to_le_bytes());
        encoded.extend_from_slice(&checksum);
        encoded.extend_from_slice(&payload);
        Ok(encoded)
    }

    pub fn decode(bytes: &[u8]) -> AndromedaResult<Self> {
        if bytes.len() < CLUSTER_MANIFEST_HEADER_LEN {
            return Err(storage_error("cluster manifest file is truncated"));
        }
        if &bytes[..CLUSTER_MANIFEST_MAGIC.len()] != CLUSTER_MANIFEST_MAGIC {
            return Err(storage_error("cluster manifest file magic mismatch"));
        }
        let mut offset = CLUSTER_MANIFEST_MAGIC.len();
        let version = read_u16_le(bytes, offset)?;
        offset += 2;
        if version != CLUSTER_MANIFEST_CODEC_VERSION_V0 {
            return Err(storage_error(
                "cluster manifest file version is unsupported",
            ));
        }
        let payload_len = usize::try_from(read_u64_le(bytes, offset)?)
            .map_err(|_| storage_error("cluster manifest payload length exceeds usize"))?;
        offset += 8;
        validate_payload_len(payload_len)?;
        let checksum = &bytes[offset..offset + CLUSTER_MANIFEST_CHECKSUM_LEN];
        offset += CLUSTER_MANIFEST_CHECKSUM_LEN;
        let expected_len = CLUSTER_MANIFEST_HEADER_LEN
            .checked_add(payload_len)
            .ok_or_else(|| storage_error("cluster manifest file length overflow"))?;
        if bytes.len() != expected_len {
            return Err(storage_error("cluster manifest file length mismatch"));
        }
        let payload = &bytes[offset..];
        let computed = Sha256::digest(payload);
        let computed_checksum: &[u8] = computed.as_ref();
        if computed_checksum != checksum {
            return Err(storage_error("cluster manifest file checksum mismatch"));
        }
        Self::decode_payload(payload)
    }

    fn encode_payload(&self) -> AndromedaResult<Vec<u8>> {
        let name = self.cluster_name.as_bytes();
        let name_len = u16::try_from(name.len())
            .map_err(|_| storage_error("cluster manifest cluster name length exceeds u16"))?;
        let member_count = u16::try_from(self.members.len())
            .map_err(|_| storage_error("cluster manifest member count exceeds u16"))?;
        let members_bytes = self
            .members
            .len()
            .checked_mul(8)
            .ok_or_else(|| storage_error("cluster manifest members length overflow"))?;
        let capacity = CLUSTER_MANIFEST_FIXED_FIELDS_BYTES
            .checked_add(name.len())
            .and_then(|len| len.checked_add(members_bytes))
            .ok_or_else(|| storage_error("cluster manifest payload length overflow"))?;
        let mut payload = Vec::with_capacity(capacity);
        payload.extend_from_slice(&self.epoch.get().to_le_bytes());
        payload.extend_from_slice(&self.manifest_version.get().to_le_bytes());
        payload.extend_from_slice(self.cluster_id.as_bytes());
        payload.extend_from_slice(&name_len.to_le_bytes());
        payload.extend_from_slice(name);
        payload.extend_from_slice(&self.primary_node_id.get().to_le_bytes());
        self.quorum_policy.encode(&mut payload);
        payload.extend_from_slice(&member_count.to_le_bytes());
        for member in &self.members {
            payload.extend_from_slice(&member.node_id.get().to_le_bytes());
        }
        payload.extend_from_slice(self.fencing_token_hash.as_bytes());
        payload.extend_from_slice(self.promotion_history_hash.as_bytes());
        payload.extend_from_slice(self.root_evidence_hash.as_bytes());
        Ok(payload)
    }

    fn decode_payload(payload: &[u8]) -> AndromedaResult<Self> {
        let mut cursor = DecodeCursor::new(payload);
        let epoch = HadrEpoch::new(cursor.read_u64()?);
        let manifest_version = HadrClusterManifestVersion::new(cursor.read_u64()?);
        let cluster_id = HadrClusterId::new(cursor.read_fixed::<16>()?);
        let name_len = usize::from(cursor.read_u16()?);
        if name_len > CLUSTER_MANIFEST_MAX_CLUSTER_NAME_BYTES {
            return Err(storage_error(
                "cluster manifest cluster name exceeds bounded length",
            ));
        }
        let cluster_name_bytes = cursor.read_bytes(name_len)?;
        let cluster_name = std::str::from_utf8(cluster_name_bytes)
            .map_err(|_| storage_error("cluster manifest cluster name is not valid utf-8"))?
            .to_owned();
        let primary_node_id = HadrNodeId::new(cursor.read_u64()?);
        let quorum_policy = HadrClusterQuorumPolicy::decode(&mut cursor)?;
        let member_count = usize::from(cursor.read_u16()?);
        if member_count > CLUSTER_MANIFEST_MAX_MEMBERS {
            return Err(storage_error(
                "cluster manifest members exceed bounded limit",
            ));
        }
        let mut members = Vec::with_capacity(member_count);
        for _ in 0..member_count {
            members.push(HadrClusterManifestMember::new(HadrNodeId::new(
                cursor.read_u64()?,
            )));
        }
        let fencing_token_hash = HadrEvidenceHash::new(cursor.read_fixed::<32>()?);
        let promotion_history_hash = HadrEvidenceHash::new(cursor.read_fixed::<32>()?);
        let root_evidence_hash = HadrEvidenceHash::new(cursor.read_fixed::<32>()?);
        if cursor.consumed() != payload.len() {
            return Err(storage_error("cluster manifest payload length mismatch"));
        }
        Self::new(
            cluster_id,
            cluster_name,
            epoch,
            manifest_version,
            primary_node_id,
            members,
            quorum_policy,
            fencing_token_hash,
            promotion_history_hash,
            root_evidence_hash,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HadrManifestPublicationEvidence {
    fencing_token_hash: HadrEvidenceHash,
    cluster_security_hash: HadrEvidenceHash,
    durable_audit_hash: HadrEvidenceHash,
    promotion_successor_hash: HadrEvidenceHash,
    root_evidence_hash: HadrEvidenceHash,
}

impl HadrManifestPublicationEvidence {
    pub const fn fencing_token_hash(&self) -> HadrEvidenceHash {
        self.fencing_token_hash
    }

    pub const fn cluster_security_hash(&self) -> HadrEvidenceHash {
        self.cluster_security_hash
    }

    pub const fn durable_audit_hash(&self) -> HadrEvidenceHash {
        self.durable_audit_hash
    }

    pub const fn promotion_successor_hash(&self) -> HadrEvidenceHash {
        self.promotion_successor_hash
    }

    pub const fn root_evidence_hash(&self) -> HadrEvidenceHash {
        self.root_evidence_hash
    }
}

/// Local, non-mutating manifest publication plan derived from a durable
/// promotion commit.
///
/// The plan does not publish the successor. It validates that the caller's
/// successor manifest strictly advances the previous manifest and that the
/// manifest-visible hashes bind the promotion's fencing token, cluster-security
/// admission digest, and durable audit receipt digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HadrManifestPublicationPlan {
    previous_epoch: HadrEpoch,
    successor_epoch: HadrEpoch,
    previous_manifest_version: HadrClusterManifestVersion,
    successor_manifest_version: HadrClusterManifestVersion,
    primary_node_id: HadrNodeId,
    evidence: HadrManifestPublicationEvidence,
}

impl HadrManifestPublicationPlan {
    pub fn from_promotion_commit(
        previous: &HadrClusterManifestV0,
        successor: &HadrClusterManifestV0,
        commit: &PromotionCommit,
        cluster_security_hash: HadrEvidenceHash,
    ) -> AndromedaResult<Self> {
        previous.validate()?;
        successor.validate()?;
        successor.ensure_advances(previous)?;
        validate_promotion_commit_successor(successor, commit)?;
        if cluster_security_hash.is_zero() {
            return Err(storage_error(
                "cluster manifest publication requires non-zero security evidence hash",
            ));
        }
        validate_cluster_security_hash(commit, cluster_security_hash)?;

        let audit_receipt = commit.audit_receipt.as_ref().ok_or_else(|| {
            storage_error("cluster manifest publication requires durable promotion audit receipt")
        })?;

        let fencing_token_hash = hash_fencing_token(commit);
        let durable_audit_hash = hash_durable_audit_receipt(audit_receipt);
        let promotion_successor_hash = hash_promotion_successor(
            previous,
            successor,
            commit,
            fencing_token_hash,
            cluster_security_hash,
            durable_audit_hash,
        );
        let root_evidence_hash = hash_manifest_publication_root(
            previous,
            successor,
            fencing_token_hash,
            cluster_security_hash,
            durable_audit_hash,
            promotion_successor_hash,
        );

        if successor.fencing_token_hash != fencing_token_hash {
            return Err(storage_error(
                "cluster manifest successor fencing token hash does not match promotion commit",
            ));
        }
        if successor.promotion_history_hash != promotion_successor_hash {
            return Err(storage_error(
                "cluster manifest successor promotion history hash does not bind publication evidence",
            ));
        }
        if successor.root_evidence_hash != root_evidence_hash {
            return Err(storage_error(
                "cluster manifest successor root evidence hash does not bind publication evidence",
            ));
        }

        Ok(Self {
            previous_epoch: previous.epoch,
            successor_epoch: successor.epoch,
            previous_manifest_version: previous.manifest_version,
            successor_manifest_version: successor.manifest_version,
            primary_node_id: successor.primary_node_id,
            evidence: HadrManifestPublicationEvidence {
                fencing_token_hash,
                cluster_security_hash,
                durable_audit_hash,
                promotion_successor_hash,
                root_evidence_hash,
            },
        })
    }

    pub const fn previous_epoch(&self) -> HadrEpoch {
        self.previous_epoch
    }

    pub const fn successor_epoch(&self) -> HadrEpoch {
        self.successor_epoch
    }

    pub const fn previous_manifest_version(&self) -> HadrClusterManifestVersion {
        self.previous_manifest_version
    }

    pub const fn successor_manifest_version(&self) -> HadrClusterManifestVersion {
        self.successor_manifest_version
    }

    pub const fn primary_node_id(&self) -> HadrNodeId {
        self.primary_node_id
    }

    pub const fn evidence(&self) -> &HadrManifestPublicationEvidence {
        &self.evidence
    }
}

pub fn derive_manifest_publication_fencing_hash(commit: &PromotionCommit) -> HadrEvidenceHash {
    hash_fencing_token(commit)
}

pub fn derive_manifest_publication_durable_audit_hash(
    commit: &PromotionCommit,
) -> AndromedaResult<HadrEvidenceHash> {
    let audit_receipt = commit.audit_receipt.as_ref().ok_or_else(|| {
        storage_error("cluster manifest publication requires durable promotion audit receipt")
    })?;
    Ok(hash_durable_audit_receipt(audit_receipt))
}

pub fn derive_manifest_publication_security_hash(
    commit: &PromotionCommit,
) -> AndromedaResult<HadrEvidenceHash> {
    let security = commit.marker.cluster_security.as_ref().ok_or_else(|| {
        storage_error("cluster manifest publication requires promotion cluster security evidence")
    })?;
    Ok(hash_cluster_security_evidence(security))
}

pub fn derive_manifest_publication_promotion_hash(
    previous: &HadrClusterManifestV0,
    successor: &HadrClusterManifestV0,
    commit: &PromotionCommit,
    cluster_security_hash: HadrEvidenceHash,
) -> AndromedaResult<HadrEvidenceHash> {
    if cluster_security_hash.is_zero() {
        return Err(storage_error(
            "cluster manifest publication requires non-zero security evidence hash",
        ));
    }
    validate_cluster_security_hash(commit, cluster_security_hash)?;
    let fencing_token_hash = hash_fencing_token(commit);
    let durable_audit_hash = derive_manifest_publication_durable_audit_hash(commit)?;
    Ok(hash_promotion_successor(
        previous,
        successor,
        commit,
        fencing_token_hash,
        cluster_security_hash,
        durable_audit_hash,
    ))
}

pub fn derive_manifest_publication_root_hash(
    previous: &HadrClusterManifestV0,
    successor: &HadrClusterManifestV0,
    commit: &PromotionCommit,
    cluster_security_hash: HadrEvidenceHash,
) -> AndromedaResult<HadrEvidenceHash> {
    if cluster_security_hash.is_zero() {
        return Err(storage_error(
            "cluster manifest publication requires non-zero security evidence hash",
        ));
    }
    validate_cluster_security_hash(commit, cluster_security_hash)?;
    let fencing_token_hash = hash_fencing_token(commit);
    let durable_audit_hash = derive_manifest_publication_durable_audit_hash(commit)?;
    let promotion_successor_hash = hash_promotion_successor(
        previous,
        successor,
        commit,
        fencing_token_hash,
        cluster_security_hash,
        durable_audit_hash,
    );
    Ok(hash_manifest_publication_root(
        previous,
        successor,
        fencing_token_hash,
        cluster_security_hash,
        durable_audit_hash,
        promotion_successor_hash,
    ))
}

fn validate_promotion_commit_successor(
    successor: &HadrClusterManifestV0,
    commit: &PromotionCommit,
) -> AndromedaResult<()> {
    if commit.token != commit.marker.token {
        return Err(storage_error(
            "cluster manifest publication requires promotion token marker consistency",
        ));
    }
    if successor.epoch != commit.token.epoch || successor.epoch != commit.marker.proposed_epoch {
        return Err(storage_error(
            "cluster manifest successor epoch must match promotion commit epoch",
        ));
    }
    if successor.primary_node_id != commit.token.primary_id
        || successor.primary_node_id != commit.marker.candidate_id
    {
        return Err(storage_error(
            "cluster manifest successor primary must match promotion commit candidate",
        ));
    }
    if commit.snapshot.epoch() != successor.epoch {
        return Err(storage_error(
            "cluster manifest publication requires committed membership epoch evidence",
        ));
    }
    let committed_primary = commit
        .snapshot
        .primary()
        .ok_or_else(|| storage_error("cluster manifest publication requires committed primary"))?;
    if committed_primary.id != successor.primary_node_id {
        return Err(storage_error(
            "cluster manifest publication primary must match committed membership snapshot",
        ));
    }
    Ok(())
}

fn hash_fencing_token(commit: &PromotionCommit) -> HadrEvidenceHash {
    let mut hasher = Sha256::new();
    hasher.update(MANIFEST_PUBLICATION_FENCING_HASH_DOMAIN);
    hasher.update(commit.token.primary_id.get().to_le_bytes());
    hasher.update(commit.token.epoch.get().to_le_bytes());
    HadrEvidenceHash::new(hasher.finalize().into())
}

fn hash_durable_audit_receipt(
    audit_receipt: &crate::promotion_boundary::HadrPromotionAuditReceipt,
) -> HadrEvidenceHash {
    let mut hasher = Sha256::new();
    hasher.update(MANIFEST_PUBLICATION_AUDIT_HASH_DOMAIN);
    hasher.update(audit_receipt.audit_lsn.get().to_le_bytes());
    hasher.update(audit_receipt.marker_digest_sha256);
    HadrEvidenceHash::new(hasher.finalize().into())
}

fn validate_cluster_security_hash(
    commit: &PromotionCommit,
    cluster_security_hash: HadrEvidenceHash,
) -> AndromedaResult<()> {
    let derived = derive_manifest_publication_security_hash(commit)?;
    if cluster_security_hash != derived {
        return Err(storage_error(
            "cluster manifest publication security evidence hash does not match promotion security evidence",
        ));
    }
    Ok(())
}

fn hash_cluster_security_evidence(security: &HadrClusterSecurityEvidence) -> HadrEvidenceHash {
    let audit = security.audit();
    let mut hasher = Sha256::new();
    hasher.update(MANIFEST_PUBLICATION_SECURITY_HASH_DOMAIN);
    hasher.update(cluster_operation_tag(security.operation()).to_le_bytes());
    hasher.update(audit.trace_id.get().to_le_bytes());
    hasher.update(audit.schema_version.get().to_le_bytes());
    hasher.update(surface_scope_tag(audit.surface).to_le_bytes());
    hash_canonical_text(&mut hasher, &audit.certificate.fingerprint);
    hash_canonical_text(&mut hasher, &audit.certificate.subject);
    hasher.update(surface_scope_tag(audit.certificate.surface).to_le_bytes());
    hash_canonical_text(&mut hasher, &audit.principal.principal_id);
    hasher.update(principal_kind_tag(audit.principal.kind).to_le_bytes());
    hasher.update(permission_tag(audit.permission).to_le_bytes());
    hasher.update(security_outcome_tag(audit.outcome).to_le_bytes());
    hasher.update(audit.policy_version.policy_version.to_le_bytes());
    hash_canonical_text(&mut hasher, &audit.policy_version.policy_digest);
    hash_canonical_text(&mut hasher, &audit.reason);
    HadrEvidenceHash::new(hasher.finalize().into())
}

fn hash_canonical_text(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn cluster_operation_tag(operation: HadrClusterOperation) -> u16 {
    match operation {
        HadrClusterOperation::PromotePrimary => 1,
        HadrClusterOperation::FenceNode => 2,
        HadrClusterOperation::UpdateManifest => 3,
    }
}

fn surface_scope_tag(surface: SurfaceScope) -> u16 {
    match surface {
        SurfaceScope::Application => 1,
        SurfaceScope::Administration => 2,
        SurfaceScope::Cluster => 3,
        SurfaceScope::BackupAgent => 4,
        SurfaceScope::MonitoringAgent => 5,
    }
}

fn principal_kind_tag(kind: UserPrincipalKind) -> u16 {
    match kind {
        UserPrincipalKind::Human => 1,
        UserPrincipalKind::Service => 2,
        UserPrincipalKind::BreakGlass => 3,
    }
}

fn permission_tag(permission: Permission) -> u16 {
    match permission {
        Permission::ExecuteProcedure => 1,
        Permission::ReadContract => 2,
        Permission::CreateTable => 3,
        Permission::CreateMap => 4,
        Permission::CreateProcedure => 5,
        Permission::ImportDefinitionBatch => 6,
        Permission::DebugProcedure => 7,
        Permission::ReadProcedureStore => 8,
        Permission::InspectPlans => 9,
        Permission::ManageSecurity => 10,
        Permission::RotateCertificate => 11,
        Permission::RevokeCertificateIdentity => 12,
        Permission::Backup => 13,
        Permission::Restore => 14,
        Permission::ForensicStart => 15,
        Permission::ClusterPromote => 16,
        Permission::FenceNode => 17,
        Permission::UpdateClusterManifest => 18,
    }
}

fn security_outcome_tag(outcome: SecurityAuditOutcome) -> u16 {
    match outcome {
        SecurityAuditOutcome::Allowed => 1,
        SecurityAuditOutcome::Denied => 2,
    }
}

fn hash_promotion_successor(
    previous: &HadrClusterManifestV0,
    successor: &HadrClusterManifestV0,
    commit: &PromotionCommit,
    fencing_token_hash: HadrEvidenceHash,
    cluster_security_hash: HadrEvidenceHash,
    durable_audit_hash: HadrEvidenceHash,
) -> HadrEvidenceHash {
    let mut hasher = Sha256::new();
    hasher.update(MANIFEST_PUBLICATION_SUCCESSOR_HASH_DOMAIN);
    hasher.update(previous.cluster_id.as_bytes());
    hasher.update(previous.epoch.get().to_le_bytes());
    hasher.update(successor.epoch.get().to_le_bytes());
    hasher.update(previous.manifest_version.get().to_le_bytes());
    hasher.update(successor.manifest_version.get().to_le_bytes());
    hasher.update(successor.primary_node_id.get().to_le_bytes());
    hasher.update(commit.marker.primary_durable_lsn.get().to_le_bytes());
    hasher.update(commit.marker.committed_safe_lsn.get().to_le_bytes());
    hasher.update(fencing_token_hash.as_bytes());
    hasher.update(cluster_security_hash.as_bytes());
    hasher.update(durable_audit_hash.as_bytes());
    HadrEvidenceHash::new(hasher.finalize().into())
}

fn hash_manifest_publication_root(
    previous: &HadrClusterManifestV0,
    successor: &HadrClusterManifestV0,
    fencing_token_hash: HadrEvidenceHash,
    cluster_security_hash: HadrEvidenceHash,
    durable_audit_hash: HadrEvidenceHash,
    promotion_successor_hash: HadrEvidenceHash,
) -> HadrEvidenceHash {
    let mut hasher = Sha256::new();
    hasher.update(MANIFEST_PUBLICATION_ROOT_HASH_DOMAIN);
    hasher.update(previous.cluster_id.as_bytes());
    hasher.update(successor.cluster_id.as_bytes());
    hasher.update(successor.epoch.get().to_le_bytes());
    hasher.update(successor.manifest_version.get().to_le_bytes());
    hasher.update(successor.primary_node_id.get().to_le_bytes());
    hasher.update(fencing_token_hash.as_bytes());
    hasher.update(cluster_security_hash.as_bytes());
    hasher.update(durable_audit_hash.as_bytes());
    hasher.update(promotion_successor_hash.as_bytes());
    HadrEvidenceHash::new(hasher.finalize().into())
}

fn validate_quorum_policy(
    policy: HadrClusterQuorumPolicy,
    member_count: usize,
) -> AndromedaResult<()> {
    match policy {
        HadrClusterQuorumPolicy::Majority => Ok(()),
        HadrClusterQuorumPolicy::Fixed { quorum_size } => {
            if quorum_size == 0 {
                return Err(storage_error(
                    "cluster manifest fixed quorum size must be non-zero",
                ));
            }
            if usize::from(quorum_size) > member_count {
                return Err(storage_error(
                    "cluster manifest fixed quorum size exceeds member count",
                ));
            }
            Ok(())
        },
    }
}

fn validate_payload_len(payload_len: usize) -> AndromedaResult<()> {
    let max_len = CLUSTER_MANIFEST_FIXED_FIELDS_BYTES
        .checked_add(CLUSTER_MANIFEST_MAX_CLUSTER_NAME_BYTES)
        .and_then(|len| len.checked_add(CLUSTER_MANIFEST_MAX_MEMBERS * 8))
        .ok_or_else(|| storage_error("cluster manifest payload length overflow"))?;
    if payload_len > max_len {
        return Err(storage_error(
            "cluster manifest payload length exceeds bounded limit",
        ));
    }
    Ok(())
}

fn read_u16_le(bytes: &[u8], offset: usize) -> AndromedaResult<u16> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| storage_error("cluster manifest header offset overflow"))?;
    let field: [u8; 2] = bytes[offset..end]
        .try_into()
        .map_err(|_| storage_error("cluster manifest header is truncated"))?;
    Ok(u16::from_le_bytes(field))
}

fn read_u64_le(bytes: &[u8], offset: usize) -> AndromedaResult<u64> {
    let end = offset
        .checked_add(8)
        .ok_or_else(|| storage_error("cluster manifest header offset overflow"))?;
    let field: [u8; 8] = bytes[offset..end]
        .try_into()
        .map_err(|_| storage_error("cluster manifest header is truncated"))?;
    Ok(u64::from_le_bytes(field))
}

fn storage_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
