use andromeda_backup::artifacts::{
    BackupArtifactDigest, BackupAuditLedgerArtifact, BackupCatalogArtifact,
};

fn valid_digest() -> BackupArtifactDigest {
    BackupArtifactDigest {
        sha256: [7; 32],
        crc64: 11,
        byte_len: 4096,
    }
}

#[test]
fn backup_catalog_artifact_validation_fails_closed() {
    let valid = BackupCatalogArtifact {
        catalog_version: 1,
        artifact: valid_digest(),
    };
    assert!(valid.validate().is_ok());

    let invalid_version = BackupCatalogArtifact {
        catalog_version: 0,
        artifact: valid_digest(),
    };
    assert!(invalid_version.validate().is_err());

    let invalid_digest = BackupCatalogArtifact {
        catalog_version: 1,
        artifact: BackupArtifactDigest {
            sha256: [0; 32],
            crc64: 11,
            byte_len: 4096,
        },
    };
    assert!(invalid_digest.validate().is_err());
}

#[test]
fn backup_audit_ledger_artifact_validation_fails_closed() {
    let valid = BackupAuditLedgerArtifact {
        ledger_epoch: 1,
        artifact: valid_digest(),
    };
    assert!(valid.validate().is_ok());

    let invalid_epoch = BackupAuditLedgerArtifact {
        ledger_epoch: 0,
        artifact: valid_digest(),
    };
    assert!(invalid_epoch.validate().is_err());

    let invalid_digest = BackupAuditLedgerArtifact {
        ledger_epoch: 1,
        artifact: BackupArtifactDigest {
            sha256: [7; 32],
            crc64: 0,
            byte_len: 4096,
        },
    };
    assert!(invalid_digest.validate().is_err());
}
