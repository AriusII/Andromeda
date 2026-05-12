use andromeda_audit::{
    CertificateIdentity, Permission, SecurityAuditOutcome, SecurityAuditTrace,
    SecurityPolicyVersionEvidence, SurfaceScope, UserPrincipal, UserPrincipalKind,
};
use andromeda_hadr::{
    HadrAuditRecord, HadrClusterId, HadrClusterManifestMember, HadrClusterManifestV0,
    HadrClusterManifestVersion, HadrClusterOperation, HadrClusterQuorumPolicy,
    HadrClusterSecurityEvidence, HadrEpoch, HadrEvidenceHash, HadrFencingToken,
    HadrManifestPublicationPlan, HadrMembershipNode, HadrMembershipSnapshot, HadrNodeId,
    HadrNodeRole, HadrPromotionAuditMarker, HadrPromotionAuditReceipt, HadrPromotionOutcome,
    PromotionCommit, derive_manifest_publication_fencing_hash,
    derive_manifest_publication_promotion_hash, derive_manifest_publication_root_hash,
    derive_manifest_publication_security_hash,
};
use andromeda_observability::TraceId;
use andromeda_wal::Lsn;

fn manifest_fixture() -> HadrClusterManifestV0 {
    HadrClusterManifestV0::new(
        HadrClusterId::new([1; 16]),
        "alpha-cluster",
        HadrEpoch::new(7),
        HadrClusterManifestVersion::new(4),
        HadrNodeId::new(2),
        vec![
            HadrClusterManifestMember::new(HadrNodeId::new(1)),
            HadrClusterManifestMember::new(HadrNodeId::new(2)),
            HadrClusterManifestMember::new(HadrNodeId::new(3)),
        ],
        HadrClusterQuorumPolicy::Majority,
        HadrEvidenceHash::new([0xA1; 32]),
        HadrEvidenceHash::new([0xB2; 32]),
        HadrEvidenceHash::new([0xC3; 32]),
    )
    .expect("valid manifest fixture")
}

fn promotion_security() -> HadrClusterSecurityEvidence {
    let surface = SurfaceScope::Cluster;
    let audit = SecurityAuditTrace::new_with_policy_version(
        TraceId::new(24_002),
        surface,
        CertificateIdentity::new("fp-manifest-publisher", "CN=hadr-controller", surface)
            .expect("certificate"),
        UserPrincipal::new("svc-hadr-controller", UserPrincipalKind::Service).expect("principal"),
        Permission::ClusterPromote,
        SecurityAuditOutcome::Allowed,
        SecurityPolicyVersionEvidence::new(
            24,
            "sha256:2424242424242424242424242424242424242424242424242424242424242424",
        )
        .expect("policy evidence"),
        "authorized HADR promotion for manifest publication",
    )
    .expect("security audit");
    HadrClusterSecurityEvidence::new(HadrClusterOperation::PromotePrimary, audit)
        .expect("cluster promotion security")
}

fn promotion_commit_fixture() -> PromotionCommit {
    let token = HadrFencingToken::new(HadrNodeId::new(2), HadrEpoch::new(8));
    let audit_record = HadrAuditRecord {
        candidate: HadrNodeId::new(2),
        candidate_role: HadrNodeRole::Replica,
        candidate_safe_lsn: Lsn::new(100),
        candidate_divergence_lsn: None,
        proposed_epoch: HadrEpoch::new(8),
        members: vec![HadrNodeId::new(1), HadrNodeId::new(2), HadrNodeId::new(3)],
        quorum_size: 2,
        granted_votes: 2,
        max_voter_safe_lsn: Lsn::new(100),
        highest_observed_epoch: HadrEpoch::new(7),
        active_token: Some(HadrFencingToken::new(HadrNodeId::new(1), HadrEpoch::new(7))),
        outcome: HadrPromotionOutcome::Approved {
            token,
            committed_safe_lsn: Lsn::new(100),
            granted_voters: vec![HadrNodeId::new(2), HadrNodeId::new(3)],
        },
    };
    let marker = HadrPromotionAuditMarker {
        candidate_id: HadrNodeId::new(2),
        proposed_epoch: HadrEpoch::new(8),
        primary_durable_lsn: Lsn::new(101),
        committed_safe_lsn: Lsn::new(100),
        token,
        audit_record,
        cluster_security: Some(promotion_security()),
    };
    let snapshot = HadrMembershipSnapshot::new(
        HadrEpoch::new(8),
        vec![
            HadrMembershipNode::new(HadrNodeId::new(1), HadrNodeRole::Replica, HadrEpoch::new(8)),
            HadrMembershipNode::new(HadrNodeId::new(2), HadrNodeRole::Primary, HadrEpoch::new(8)),
            HadrMembershipNode::new(HadrNodeId::new(3), HadrNodeRole::Replica, HadrEpoch::new(8)),
        ],
    )
    .expect("membership snapshot");

    PromotionCommit {
        token,
        marker,
        audit_receipt: Some(
            HadrPromotionAuditReceipt::new(Lsn::new(120), [0xD4; 32]).expect("audit receipt"),
        ),
        snapshot,
    }
}

fn previous_publication_manifest() -> HadrClusterManifestV0 {
    HadrClusterManifestV0::new(
        HadrClusterId::new([1; 16]),
        "alpha-cluster",
        HadrEpoch::new(7),
        HadrClusterManifestVersion::new(4),
        HadrNodeId::new(1),
        vec![
            HadrClusterManifestMember::new(HadrNodeId::new(1)),
            HadrClusterManifestMember::new(HadrNodeId::new(2)),
            HadrClusterManifestMember::new(HadrNodeId::new(3)),
        ],
        HadrClusterQuorumPolicy::Majority,
        HadrEvidenceHash::new([0xA1; 32]),
        HadrEvidenceHash::new([0xB2; 32]),
        HadrEvidenceHash::new([0xC3; 32]),
    )
    .expect("valid previous manifest")
}

fn successor_publication_manifest(
    previous: &HadrClusterManifestV0,
    commit: &PromotionCommit,
    cluster_security_hash: HadrEvidenceHash,
) -> HadrClusterManifestV0 {
    let fencing_hash = derive_manifest_publication_fencing_hash(commit);
    let draft = HadrClusterManifestV0::new(
        previous.cluster_id,
        previous.cluster_name.clone(),
        HadrEpoch::new(8),
        HadrClusterManifestVersion::new(5),
        HadrNodeId::new(2),
        previous.members.clone(),
        HadrClusterQuorumPolicy::Majority,
        fencing_hash,
        HadrEvidenceHash::new([0xE5; 32]),
        HadrEvidenceHash::new([0xF6; 32]),
    )
    .expect("draft successor manifest");
    let promotion_hash =
        derive_manifest_publication_promotion_hash(previous, &draft, commit, cluster_security_hash)
            .expect("promotion publication hash");
    let rooted = HadrClusterManifestV0::new(
        previous.cluster_id,
        previous.cluster_name.clone(),
        HadrEpoch::new(8),
        HadrClusterManifestVersion::new(5),
        HadrNodeId::new(2),
        previous.members.clone(),
        HadrClusterQuorumPolicy::Majority,
        fencing_hash,
        promotion_hash,
        HadrEvidenceHash::new([0xF6; 32]),
    )
    .expect("root draft successor manifest");
    let root_hash =
        derive_manifest_publication_root_hash(previous, &rooted, commit, cluster_security_hash)
            .expect("root publication hash");
    HadrClusterManifestV0::new(
        previous.cluster_id,
        previous.cluster_name.clone(),
        HadrEpoch::new(8),
        HadrClusterManifestVersion::new(5),
        HadrNodeId::new(2),
        previous.members.clone(),
        HadrClusterQuorumPolicy::Majority,
        fencing_hash,
        promotion_hash,
        root_hash,
    )
    .expect("successor manifest")
}

#[test]
fn cluster_manifest_roundtrip_preserves_fields() {
    let manifest = manifest_fixture();
    let encoded = manifest.encode().expect("encode manifest");
    let decoded = HadrClusterManifestV0::decode(&encoded).expect("decode manifest");
    assert_eq!(decoded, manifest);
}

#[test]
fn cluster_manifest_codec_bytes_have_stable_layout_contract() {
    let manifest = manifest_fixture();
    let encoded = manifest.encode().expect("encode manifest");

    assert_eq!(&encoded[..16], b"AND-HADR-CMANV0\0");
    assert_eq!(u16::from_le_bytes([encoded[16], encoded[17]]), 1);
    assert_eq!(
        u64::from_le_bytes([
            encoded[18],
            encoded[19],
            encoded[20],
            encoded[21],
            encoded[22],
            encoded[23],
            encoded[24],
            encoded[25]
        ]),
        180
    );

    let payload = &encoded[58..];
    assert_eq!(
        u64::from_le_bytes([
            payload[0], payload[1], payload[2], payload[3], payload[4], payload[5], payload[6],
            payload[7]
        ]),
        7
    );
    assert_eq!(
        u64::from_le_bytes([
            payload[8],
            payload[9],
            payload[10],
            payload[11],
            payload[12],
            payload[13],
            payload[14],
            payload[15]
        ]),
        4
    );
}

#[test]
fn cluster_manifest_decode_rejects_bad_magic() {
    let mut encoded = manifest_fixture().encode().expect("encode manifest");
    encoded[0] ^= 0xFF;
    let error = HadrClusterManifestV0::decode(&encoded).expect_err("bad magic must fail");
    assert!(error.message().contains("magic mismatch"));
}

#[test]
fn cluster_manifest_decode_rejects_unsupported_version() {
    let mut encoded = manifest_fixture().encode().expect("encode manifest");
    encoded[16..18].copy_from_slice(&2u16.to_le_bytes());
    let error = HadrClusterManifestV0::decode(&encoded).expect_err("unsupported version must fail");
    assert!(error.message().contains("version is unsupported"));
}

#[test]
fn cluster_manifest_decode_rejects_truncated_file() {
    let encoded = manifest_fixture().encode().expect("encode manifest");
    let truncated = &encoded[..encoded.len() - 1];
    let error = HadrClusterManifestV0::decode(truncated).expect_err("truncation must fail");
    assert!(error.message().contains("length mismatch"));
}

#[test]
fn cluster_manifest_decode_rejects_checksum_mismatch() {
    let mut encoded = manifest_fixture().encode().expect("encode manifest");
    let last = encoded.last_mut().expect("encoded bytes");
    *last ^= 0x11;
    let error =
        HadrClusterManifestV0::decode(&encoded).expect_err("checksum mismatch must fail decode");
    assert!(error.message().contains("checksum mismatch"));
}

#[test]
fn cluster_manifest_decode_rejects_payload_length_mismatch() {
    let mut encoded = manifest_fixture().encode().expect("encode manifest");
    encoded[18..26].copy_from_slice(&0u64.to_le_bytes());
    let error = HadrClusterManifestV0::decode(&encoded).expect_err("payload length mismatch");
    assert!(error.message().contains("length mismatch"));
}

#[test]
fn cluster_manifest_validate_rejects_duplicate_member_ids() {
    let error = HadrClusterManifestV0::new(
        HadrClusterId::new([1; 16]),
        "alpha-cluster",
        HadrEpoch::new(1),
        HadrClusterManifestVersion::new(1),
        HadrNodeId::new(1),
        vec![
            HadrClusterManifestMember::new(HadrNodeId::new(1)),
            HadrClusterManifestMember::new(HadrNodeId::new(1)),
        ],
        HadrClusterQuorumPolicy::Majority,
        HadrEvidenceHash::new([0xA1; 32]),
        HadrEvidenceHash::new([0xB2; 32]),
        HadrEvidenceHash::new([0xC3; 32]),
    )
    .expect_err("duplicate member ids must fail");
    assert!(error.message().contains("must be unique"));
}

#[test]
fn cluster_manifest_validate_rejects_invalid_primary_reference() {
    let error = HadrClusterManifestV0::new(
        HadrClusterId::new([1; 16]),
        "alpha-cluster",
        HadrEpoch::new(1),
        HadrClusterManifestVersion::new(1),
        HadrNodeId::new(9),
        vec![
            HadrClusterManifestMember::new(HadrNodeId::new(1)),
            HadrClusterManifestMember::new(HadrNodeId::new(2)),
        ],
        HadrClusterQuorumPolicy::Majority,
        HadrEvidenceHash::new([0xA1; 32]),
        HadrEvidenceHash::new([0xB2; 32]),
        HadrEvidenceHash::new([0xC3; 32]),
    )
    .expect_err("invalid primary reference must fail");
    assert!(error.message().contains("must reference a member"));
}

#[test]
fn cluster_manifest_validate_rejects_missing_fencing_hash() {
    let error = HadrClusterManifestV0::new(
        HadrClusterId::new([1; 16]),
        "alpha-cluster",
        HadrEpoch::new(1),
        HadrClusterManifestVersion::new(1),
        HadrNodeId::new(1),
        vec![HadrClusterManifestMember::new(HadrNodeId::new(1))],
        HadrClusterQuorumPolicy::Majority,
        HadrEvidenceHash::new([0; 32]),
        HadrEvidenceHash::new([0xB2; 32]),
        HadrEvidenceHash::new([0xC3; 32]),
    )
    .expect_err("missing fencing hash must fail");
    assert!(error.message().contains("fencing token hash"));
}

#[test]
fn cluster_manifest_successor_rejects_stale_epoch_or_version() {
    let previous = manifest_fixture();
    let stale = HadrClusterManifestV0::new(
        HadrClusterId::new([1; 16]),
        "alpha-cluster",
        HadrEpoch::new(7),
        HadrClusterManifestVersion::new(4),
        HadrNodeId::new(2),
        vec![
            HadrClusterManifestMember::new(HadrNodeId::new(1)),
            HadrClusterManifestMember::new(HadrNodeId::new(2)),
        ],
        HadrClusterQuorumPolicy::Fixed { quorum_size: 2 },
        HadrEvidenceHash::new([0xA2; 32]),
        HadrEvidenceHash::new([0xB3; 32]),
        HadrEvidenceHash::new([0xC4; 32]),
    )
    .expect("valid stale manifest fixture");

    let error = stale
        .ensure_advances(&previous)
        .expect_err("successor with stale epoch/version must fail");
    assert!(error.message().contains("strictly advance"));
}

#[test]
fn promotion_manifest_publication_plan_advances_and_binds_evidence_hashes() {
    let previous = previous_publication_manifest();
    let commit = promotion_commit_fixture();
    let cluster_security_hash =
        derive_manifest_publication_security_hash(&commit).expect("security hash");
    let successor = successor_publication_manifest(&previous, &commit, cluster_security_hash);

    let plan = HadrManifestPublicationPlan::from_promotion_commit(
        &previous,
        &successor,
        &commit,
        cluster_security_hash,
    )
    .expect("promotion-derived manifest publication plan");

    assert_eq!(plan.previous_epoch(), HadrEpoch::new(7));
    assert_eq!(plan.successor_epoch(), HadrEpoch::new(8));
    assert_eq!(
        plan.previous_manifest_version(),
        HadrClusterManifestVersion::new(4)
    );
    assert_eq!(
        plan.successor_manifest_version(),
        HadrClusterManifestVersion::new(5)
    );
    assert_eq!(plan.primary_node_id(), HadrNodeId::new(2));
    assert_eq!(
        successor.fencing_token_hash,
        plan.evidence().fencing_token_hash()
    );
    assert_eq!(
        successor.promotion_history_hash,
        plan.evidence().promotion_successor_hash()
    );
    assert_eq!(
        successor.root_evidence_hash,
        plan.evidence().root_evidence_hash()
    );
    assert_eq!(
        plan.evidence().cluster_security_hash(),
        cluster_security_hash
    );
    assert!(!plan.evidence().durable_audit_hash().is_zero());
}

#[test]
fn promotion_manifest_publication_security_hash_has_stable_canonical_vector() {
    let commit = promotion_commit_fixture();
    let cluster_security_hash =
        derive_manifest_publication_security_hash(&commit).expect("security hash");

    assert_eq!(
        cluster_security_hash.as_bytes(),
        &[
            0x33, 0x88, 0xD1, 0x64, 0xA3, 0x64, 0xB1, 0x09, 0xE2, 0xDF, 0xE9, 0xC5, 0x3B, 0x65,
            0xB5, 0x9B, 0x7D, 0x13, 0x57, 0x6F, 0x2D, 0x1E, 0x6D, 0xAF, 0xC7, 0x21, 0xD1, 0x87,
            0xD5, 0x02, 0x39, 0x74,
        ]
    );
}

#[test]
fn promotion_manifest_publication_rejects_unbound_security_hash() {
    let previous = previous_publication_manifest();
    let commit = promotion_commit_fixture();
    let cluster_security_hash =
        derive_manifest_publication_security_hash(&commit).expect("security hash");
    let successor = successor_publication_manifest(&previous, &commit, cluster_security_hash);

    let error = HadrManifestPublicationPlan::from_promotion_commit(
        &previous,
        &successor,
        &commit,
        HadrEvidenceHash::new([0x43; 32]),
    )
    .expect_err("wrong security hash must not validate successor publication");

    assert!(error.message().contains("security evidence hash"));
}
