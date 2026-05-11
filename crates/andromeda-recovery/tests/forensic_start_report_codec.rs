//! Round-trip codec tests for [`ForensicStartReport`] v0.
//!
//! Codec contract:
//! - Magic bytes: `b"FSRP"` (`FORENSIC_REPORT_MAGIC`).
//! - Version tag: `0` (`FORENSIC_REPORT_CODEC_VERSION_V0`).
//! - All multi-byte fields are little-endian.
//! - No native Rust struct layout is written to disk.
//!
//! The golden byte vector is computed from a known fixture and verified to
//! survive a round-trip through `encode_v0` → `decode_v0`.  This test also
//! asserts that wrong magic and wrong version are rejected.

use andromeda_recovery::{
    FORENSIC_REPORT_CODEC_VERSION_V0, FORENSIC_REPORT_MAGIC, ForensicAnomalyKind,
    ForensicAnomalyReport, ForensicCatalogSection, ForensicIndexSection, ForensicMapRefreshState,
    ForensicMapsSection, ForensicSecurityAuditSection, ForensicSnapshotSection,
    ForensicStartReport, ForensicValidationState, ForensicWalSection,
};
use andromeda_wal::{WalScanStop, WalScanStopReason};

// ── Codec-version constants ───────────────────────────────────────────────────

/// Codec version tag written into the stream.
const CODEC_V0: u8 = FORENSIC_REPORT_CODEC_VERSION_V0;

// ── Fixtures ──────────────────────────────────────────────────────────────────

fn fixture_report_clean() -> ForensicStartReport {
    ForensicStartReport {
        catalog: ForensicCatalogSection {
            catalog_version: 3,
            object_count: 128,
            validation_state: ForensicValidationState::Validated,
        },
        wal: ForensicWalSection {
            coverage_start_lsn: 1,
            coverage_end_lsn: 500,
            anomaly: None,
            scan_complete: true,
        },
        snapshot: ForensicSnapshotSection {
            snapshot_id: 7,
            snapshot_hash: [0xCC; 32],
            manifest_validated: true,
        },
        indexes: ForensicIndexSection {
            segment_index_count: 12,
            segment_index_validated: true,
            last_segment_id: 12,
        },
        maps: ForensicMapsSection {
            map_count: 4,
            refresh_plan_state: ForensicMapRefreshState::Idle,
            staleness_max_lsn_lag: 0,
        },
        security_audit: ForensicSecurityAuditSection {
            last_audit_lsn: 490,
            retention_boundary_satisfied: true,
            audit_record_count: 2_048,
        },
        generated_at_epoch: 1_700_000_001,
    }
}

fn fixture_report_with_anomaly() -> ForensicStartReport {
    use andromeda_recovery::ForensicAnomaly;

    ForensicStartReport {
        catalog: ForensicCatalogSection {
            catalog_version: 9,
            object_count: 64,
            validation_state: ForensicValidationState::Validated,
        },
        wal: ForensicWalSection {
            coverage_start_lsn: 100,
            coverage_end_lsn: 800,
            anomaly: Some(ForensicAnomalyReport {
                anomalies: vec![ForensicAnomaly {
                    kind: ForensicAnomalyKind::LsnChainBreak { offset: 42 },
                    detail: "chain break at offset 42".into(),
                }],
                scan_stop: Some(WalScanStop {
                    offset: 42,
                    reason: WalScanStopReason::LsnGap,
                }),
            }),
            scan_complete: true,
        },
        snapshot: ForensicSnapshotSection {
            snapshot_id: 11,
            snapshot_hash: [0xDD; 32],
            manifest_validated: true,
        },
        indexes: ForensicIndexSection {
            segment_index_count: 3,
            segment_index_validated: true,
            last_segment_id: 3,
        },
        maps: ForensicMapsSection {
            map_count: 1,
            refresh_plan_state: ForensicMapRefreshState::RefreshPending,
            staleness_max_lsn_lag: 10,
        },
        security_audit: ForensicSecurityAuditSection {
            last_audit_lsn: 790,
            retention_boundary_satisfied: true,
            audit_record_count: 512,
        },
        generated_at_epoch: 1_700_000_099,
    }
}

fn fixture_report_failed_catalog() -> ForensicStartReport {
    let mut r = fixture_report_clean();
    r.catalog.validation_state = ForensicValidationState::Failed("catalog drift detected".into());
    r
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ── Round-trip tests ──────────────────────────────────────────────────────────

#[test]
fn clean_report_survives_encode_decode_v0_round_trip() {
    let original = fixture_report_clean();
    let encoded = original.encode_v0();
    let decoded = ForensicStartReport::decode_v0(&encoded)
        .expect("clean report must round-trip through encode_v0/decode_v0");

    assert_eq!(
        original, decoded,
        "round-trip must produce identical ForensicStartReport"
    );
}

#[test]
fn report_with_anomaly_survives_encode_decode_v0_round_trip() {
    let original = fixture_report_with_anomaly();
    let encoded = original.encode_v0();
    let decoded = ForensicStartReport::decode_v0(&encoded)
        .expect("report with anomaly must round-trip through encode_v0/decode_v0");

    assert_eq!(original, decoded, "round-trip must preserve anomaly data");
    let wal = decoded.wal.anomaly.as_ref().unwrap();
    assert_eq!(wal.anomalies.len(), 1);
    assert_eq!(
        wal.anomalies[0].kind,
        ForensicAnomalyKind::LsnChainBreak { offset: 42 }
    );
    assert_eq!(wal.scan_stop.unwrap().reason, WalScanStopReason::LsnGap);
}

#[test]
fn failed_catalog_state_survives_encode_decode_v0_round_trip() {
    let original = fixture_report_failed_catalog();
    let encoded = original.encode_v0();
    let decoded = ForensicStartReport::decode_v0(&encoded)
        .expect("report with Failed catalog must round-trip");

    assert_eq!(original, decoded);
    assert!(matches!(
        decoded.catalog.validation_state,
        ForensicValidationState::Failed(ref msg) if msg == "catalog drift detected"
    ));
}

// ── Magic and version guard tests ─────────────────────────────────────────────

#[test]
fn decode_rejects_wrong_magic() {
    let original = fixture_report_clean();
    let mut encoded = original.encode_v0();
    // Corrupt magic bytes
    encoded[0] = 0xFF;
    encoded[1] = 0xFF;

    let result = ForensicStartReport::decode_v0(&encoded);
    assert!(
        result.is_err(),
        "decode_v0 must reject bytes with wrong magic"
    );
    let err = result.unwrap_err();
    assert!(
        err.message().contains("magic"),
        "error must mention magic: {:?}",
        err.message()
    );
}

#[test]
fn decode_rejects_unsupported_codec_version() {
    let original = fixture_report_clean();
    let mut encoded = original.encode_v0();
    // Version byte is at index 4 (after 4 magic bytes)
    encoded[4] = 99;

    let result = ForensicStartReport::decode_v0(&encoded);
    assert!(
        result.is_err(),
        "decode_v0 must reject an unknown codec version"
    );
    let err = result.unwrap_err();
    assert!(
        err.message().contains("unsupported"),
        "error must mention unsupported version: {:?}",
        err.message()
    );
}

#[test]
fn decode_rejects_truncated_bytes() {
    let original = fixture_report_clean();
    let encoded = original.encode_v0();
    // Truncate to just magic + version — body missing
    let truncated = &encoded[..5];

    let result = ForensicStartReport::decode_v0(truncated);
    assert!(
        result.is_err(),
        "decode_v0 must reject truncated byte slice"
    );
}

#[test]
fn decode_rejects_trailing_bytes_after_valid_report() {
    let mut encoded = fixture_report_clean().encode_v0();
    encoded.extend_from_slice(&[0xAA, 0xBB, 0xCC]);

    let result = ForensicStartReport::decode_v0(&encoded);
    assert!(
        result.is_err(),
        "decode_v0 must reject trailing bytes beyond the canonical payload"
    );
    let err = result.unwrap_err();
    assert!(
        err.message().contains("trailing bytes"),
        "error must mention trailing bytes: {:?}",
        err.message()
    );
}

#[test]
fn decode_rejects_invalid_has_anomaly_tag() {
    let mut encoded = fixture_report_clean().encode_v0();
    // Byte 39 is wal.has_anomaly in the v0 clean fixture layout.
    encoded[39] = 2;

    let result = ForensicStartReport::decode_v0(&encoded);
    assert!(
        result.is_err(),
        "decode_v0 must reject has_anomaly values outside {{0,1}}"
    );
    let err = result.unwrap_err();
    assert!(
        err.message().contains("has_anomaly"),
        "error must mention has_anomaly: {:?}",
        err.message()
    );
}

#[test]
fn decode_rejects_invalid_has_scan_stop_tag() {
    let mut encoded = fixture_report_with_anomaly().encode_v0();
    let detail = b"chain break at offset 42";
    let detail_start = find_subslice(&encoded, detail).expect("fixture detail must be present");
    let has_scan_stop_index = detail_start + detail.len();
    encoded[has_scan_stop_index] = 2;

    let result = ForensicStartReport::decode_v0(&encoded);
    assert!(
        result.is_err(),
        "decode_v0 must reject has_scan_stop values outside {{0,1}}"
    );
    let err = result.unwrap_err();
    assert!(
        err.message().contains("has_scan_stop"),
        "error must mention has_scan_stop: {:?}",
        err.message()
    );
}

// ── Magic / version constant assertions ───────────────────────────────────────

#[test]
fn codec_constants_are_stable() {
    assert_eq!(
        FORENSIC_REPORT_MAGIC, *b"FSRP",
        "FORENSIC_REPORT_MAGIC must be b\"FSRP\""
    );
    assert_eq!(CODEC_V0, 0, "FORENSIC_REPORT_CODEC_VERSION_V0 must be 0");
}

#[test]
fn encoded_bytes_start_with_magic_and_version() {
    let report = fixture_report_clean();
    let encoded = report.encode_v0();

    assert!(
        encoded.len() > 5,
        "encoded report must be longer than magic + version"
    );
    assert_eq!(&encoded[..4], b"FSRP", "first 4 bytes must be FSRP magic");
    assert_eq!(encoded[4], CODEC_V0, "fifth byte must be codec version 0");
}

#[test]
fn clean_fixture_matches_fixed_full_byte_golden_vector() {
    let encoded = fixture_report_clean().encode_v0();
    let expected: [u8; 140] = [
        0x46, 0x53, 0x52, 0x50, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0xF4, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
        0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
        0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0x01, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
        0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xEA, 0x01, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0xF1, 0x53,
        0x65, 0x00, 0x00, 0x00, 0x00,
    ];
    assert_eq!(encoded, expected, "fixture bytes must remain stable for v0");
    let decoded =
        ForensicStartReport::decode_v0(&expected).expect("golden vector must decode as v0");
    assert_eq!(decoded, fixture_report_clean());
}
