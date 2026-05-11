//! ForensicStartReport v0 — aggregated forensic consistency proof.
//!
//! This module provides the six-section report required by P13 exit criterion
//! line 38: "ForensicStartReport v0".  All sections are REQUIRED for v0; an emitter
//! must populate every section before calling [`ForensicStartReport::all_sections_validated`].
//!
//! # C5 codec contract
//!
//! [`ForensicStartReport::encode_v0`] and [`ForensicStartReport::decode_v0`] use
//! explicit LE push helpers (`push_u64_le`, `push_bytes`, …); no native Rust struct
//! layout is ever written to disk.  Codec version tag = `0` (constant
//! [`FORENSIC_REPORT_CODEC_VERSION_V0`]).

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_wal::{WalScanStop, WalScanStopReason};

use crate::forensic_start::{ForensicAnomaly, ForensicAnomalyKind, ForensicAnomalyReport};

// ── Codec constants ───────────────────────────────────────────────────────────

/// Four-byte magic identifying a `ForensicStartReport` byte stream.
pub const FORENSIC_REPORT_MAGIC: [u8; 4] = *b"FSRP";

/// v0 codec version tag written as the fifth byte of every encoded report.
pub const FORENSIC_REPORT_CODEC_VERSION_V0: u8 = 0;

// ── State enumerations ────────────────────────────────────────────────────────

/// Validation outcome for a catalog or section consistency check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForensicValidationState {
    /// Validation has not yet been attempted.
    Pending,
    /// Validation completed successfully.
    Validated,
    /// Validation failed with a human-readable reason.
    Failed(String),
}

/// Operational state of the map refresh plan at forensic-start time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForensicMapRefreshState {
    /// No refresh required or in progress.
    Idle,
    /// A refresh has been scheduled but not yet applied.
    RefreshPending,
    /// Maps are stale beyond an acceptable threshold — validation fails closed.
    Stale,
}

// ── Section types ─────────────────────────────────────────────────────────────

/// Forensic consistency snapshot of the catalog at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForensicCatalogSection {
    pub catalog_version: u64,
    pub object_count: u64,
    pub validation_state: ForensicValidationState,
}

impl ForensicCatalogSection {
    /// Fail-closed: `Pending` and `Failed` states are validation errors.
    pub fn validate(&self) -> AndromedaResult<()> {
        match &self.validation_state {
            ForensicValidationState::Validated => Ok(()),
            ForensicValidationState::Pending => Err(section_error(
                "forensic catalog section validation_state is Pending; \
                 state must be Validated before report acceptance",
            )),
            ForensicValidationState::Failed(reason) => Err(section_error(format!(
                "forensic catalog section validation failed: {reason}"
            ))),
        }
    }
}

/// Forensic WAL coverage snapshot, reusing the existing [`ForensicAnomalyReport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForensicWalSection {
    /// LSN at the start of durable WAL coverage, as a raw `u64`.
    pub coverage_start_lsn: u64,
    /// LSN at the end of durable WAL coverage, as a raw `u64`.
    pub coverage_end_lsn: u64,
    /// Optional anomaly report from the WAL scan.  `None` means a clean scan.
    pub anomaly: Option<ForensicAnomalyReport>,
    /// `true` iff the WAL scan ran to completion without a hard stop.
    ///
    /// A stopped scan is valid only when paired with a concrete anomaly report.
    pub scan_complete: bool,
}

impl ForensicWalSection {
    /// Fail-closed: an incomplete WAL scan without anomaly evidence is invalid.
    pub fn validate(&self) -> AndromedaResult<()> {
        if !self.scan_complete && self.anomaly.as_ref().and_then(|a| a.scan_stop).is_none() {
            return Err(section_error(
                "forensic WAL section scan_complete is false; \
                 incomplete scans require bound anomaly evidence before report acceptance",
            ));
        }
        Ok(())
    }
}

/// Forensic consistency snapshot of the cold snapshot and manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForensicSnapshotSection {
    pub snapshot_id: u64,
    pub snapshot_hash: [u8; 32],
    pub manifest_validated: bool,
}

impl ForensicSnapshotSection {
    /// Fail-closed: zero snapshot_id or unvalidated manifest are errors.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.snapshot_id == 0 {
            return Err(section_error(
                "forensic snapshot section snapshot_id is zero; \
                 durable snapshot identity is required",
            ));
        }
        if !self.manifest_validated {
            return Err(section_error(
                "forensic snapshot section manifest_validated is false; \
                 manifest must be validated before report acceptance",
            ));
        }
        Ok(())
    }
}

/// Forensic consistency snapshot of the segment index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForensicIndexSection {
    pub segment_index_count: u64,
    pub segment_index_validated: bool,
    pub last_segment_id: u64,
}

impl ForensicIndexSection {
    /// Fail-closed: an unvalidated segment index is an error.
    pub fn validate(&self) -> AndromedaResult<()> {
        if !self.segment_index_validated {
            return Err(section_error(
                "forensic index section segment_index_validated is false; \
                 segment index must be validated before report acceptance",
            ));
        }
        Ok(())
    }
}

/// Forensic consistency snapshot of the maps subsystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForensicMapsSection {
    pub map_count: u64,
    pub refresh_plan_state: ForensicMapRefreshState,
    /// Maximum LSN lag across all maps relative to the WAL high watermark.
    pub staleness_max_lsn_lag: u64,
}

impl ForensicMapsSection {
    /// Fail-closed: `Stale` maps are a validation error.
    pub fn validate(&self) -> AndromedaResult<()> {
        if matches!(self.refresh_plan_state, ForensicMapRefreshState::Stale) {
            return Err(section_error(
                "forensic maps section refresh_plan_state is Stale; \
                 maps must not be stale at forensic report acceptance",
            ));
        }
        Ok(())
    }
}

/// Forensic consistency snapshot of the security audit ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForensicSecurityAuditSection {
    /// LSN of the last durable audit record observed.
    pub last_audit_lsn: u64,
    /// `true` iff the audit retention boundary invariant is satisfied.
    pub retention_boundary_satisfied: bool,
    pub audit_record_count: u64,
}

impl ForensicSecurityAuditSection {
    /// Fail-closed: an unsatisfied retention boundary is an error.
    pub fn validate(&self) -> AndromedaResult<()> {
        if !self.retention_boundary_satisfied {
            return Err(section_error(
                "forensic security audit section retention_boundary_satisfied is false; \
                 audit retention boundary is required before report acceptance",
            ));
        }
        Ok(())
    }
}

// ── Aggregate report ──────────────────────────────────────────────────────────

/// Aggregated forensic consistency proof required by P13 exit criterion line 38.
///
/// All six sections are REQUIRED for v0. [`all_sections_validated`] must return
/// `true` before this report may be presented to `forensic_start_with_report`.
///
/// [`all_sections_validated`]: ForensicStartReport::all_sections_validated
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForensicStartReport {
    pub catalog: ForensicCatalogSection,
    pub wal: ForensicWalSection,
    pub snapshot: ForensicSnapshotSection,
    pub indexes: ForensicIndexSection,
    pub maps: ForensicMapsSection,
    pub security_audit: ForensicSecurityAuditSection,
    /// Unix epoch seconds at the moment this report was generated.
    pub generated_at_epoch: u64,
}

impl ForensicStartReport {
    /// Returns `true` only when every section's [`validate()`] passes.
    ///
    /// Callers MUST check this before presenting the report to
    /// `forensic_start_with_report`.
    ///
    /// [`validate()`]: ForensicCatalogSection::validate
    pub fn all_sections_validated(&self) -> bool {
        self.catalog.validate().is_ok()
            && self.wal.validate().is_ok()
            && self.snapshot.validate().is_ok()
            && self.indexes.validate().is_ok()
            && self.maps.validate().is_ok()
            && self.security_audit.validate().is_ok()
    }

    /// Encode this report using the v0 LE binary codec.
    ///
    /// Format (all multi-byte integers are little-endian):
    /// ```text
    /// magic[4] || codec_version[1] || catalog_section || wal_section ||
    /// snapshot_section || index_section || maps_section ||
    /// security_audit_section || generated_at_epoch[8]
    /// ```
    ///
    /// No native Rust struct layout is written.  The codec version tag is
    /// [`FORENSIC_REPORT_CODEC_VERSION_V0`] = `0`.
    pub fn encode_v0(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        push_bytes(&mut buf, &FORENSIC_REPORT_MAGIC);
        push_u8(&mut buf, FORENSIC_REPORT_CODEC_VERSION_V0);
        encode_catalog_section(&mut buf, &self.catalog);
        encode_wal_section(&mut buf, &self.wal);
        encode_snapshot_section(&mut buf, &self.snapshot);
        encode_index_section(&mut buf, &self.indexes);
        encode_maps_section(&mut buf, &self.maps);
        encode_security_audit_section(&mut buf, &self.security_audit);
        push_u64_le(&mut buf, self.generated_at_epoch);
        buf
    }

    /// Decode a report from bytes previously produced by [`encode_v0`].
    ///
    /// [`encode_v0`]: ForensicStartReport::encode_v0
    pub fn decode_v0(bytes: &[u8]) -> AndromedaResult<Self> {
        let mut pos: usize = 0;

        let magic: [u8; 4] = read_fixed(bytes, &mut pos, "FSRP magic")?;
        if magic != FORENSIC_REPORT_MAGIC {
            return Err(codec_error("forensic report magic bytes mismatch"));
        }
        let version = read_u8(bytes, &mut pos, "codec_version")?;
        if version != FORENSIC_REPORT_CODEC_VERSION_V0 {
            return Err(codec_error(format!(
                "unsupported forensic report codec version {version}; \
                 expected {FORENSIC_REPORT_CODEC_VERSION_V0}"
            )));
        }

        let catalog = decode_catalog_section(bytes, &mut pos)?;
        let wal = decode_wal_section(bytes, &mut pos)?;
        let snapshot = decode_snapshot_section(bytes, &mut pos)?;
        let indexes = decode_index_section(bytes, &mut pos)?;
        let maps = decode_maps_section(bytes, &mut pos)?;
        let security_audit = decode_security_audit_section(bytes, &mut pos)?;
        let generated_at_epoch = read_u64_le(bytes, &mut pos, "generated_at_epoch")?;
        if pos != bytes.len() {
            return Err(codec_error(format!(
                "forensic report trailing bytes: decoded {pos} of {} bytes",
                bytes.len()
            )));
        }

        Ok(Self {
            catalog,
            wal,
            snapshot,
            indexes,
            maps,
            security_audit,
            generated_at_epoch,
        })
    }
}

// ── LE push helpers ───────────────────────────────────────────────────────────

fn push_u8(buf: &mut Vec<u8>, value: u8) {
    buf.push(value);
}

fn push_u32_le(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

fn push_u64_le(buf: &mut Vec<u8>, value: u64) {
    buf.extend_from_slice(&value.to_le_bytes());
}

fn push_bytes(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(bytes);
}

fn push_string(buf: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    push_u32_le(buf, b.len() as u32);
    push_bytes(buf, b);
}

// ── LE read helpers ───────────────────────────────────────────────────────────

fn read_u8(bytes: &[u8], pos: &mut usize, field: &'static str) -> AndromedaResult<u8> {
    let end = pos
        .checked_add(1)
        .ok_or_else(|| codec_error(format!("{field}: position overflow")))?;
    let byte = *bytes
        .get(*pos)
        .ok_or_else(|| codec_error(format!("{field}: truncated at byte {pos}")))?;
    *pos = end;
    Ok(byte)
}

fn read_u32_le(bytes: &[u8], pos: &mut usize, field: &'static str) -> AndromedaResult<u32> {
    let end = pos
        .checked_add(4)
        .ok_or_else(|| codec_error(format!("{field}: position overflow")))?;
    let slice = bytes
        .get(*pos..end)
        .ok_or_else(|| codec_error(format!("{field}: truncated at {pos}..{end}")))?;
    *pos = end;
    Ok(u32::from_le_bytes(slice.try_into().map_err(|_| {
        codec_error(format!("{field}: read_u32 conversion"))
    })?))
}

fn read_u64_le(bytes: &[u8], pos: &mut usize, field: &'static str) -> AndromedaResult<u64> {
    let end = pos
        .checked_add(8)
        .ok_or_else(|| codec_error(format!("{field}: position overflow")))?;
    let slice = bytes
        .get(*pos..end)
        .ok_or_else(|| codec_error(format!("{field}: truncated at {pos}..{end}")))?;
    *pos = end;
    Ok(u64::from_le_bytes(slice.try_into().map_err(|_| {
        codec_error(format!("{field}: read_u64 conversion"))
    })?))
}

fn read_fixed<const N: usize>(
    bytes: &[u8],
    pos: &mut usize,
    field: &'static str,
) -> AndromedaResult<[u8; N]> {
    let end = pos
        .checked_add(N)
        .ok_or_else(|| codec_error(format!("{field}: position overflow")))?;
    let slice = bytes
        .get(*pos..end)
        .ok_or_else(|| codec_error(format!("{field}: truncated at {pos}..{end}")))?;
    *pos = end;
    slice
        .try_into()
        .map_err(|_| codec_error(format!("{field}: fixed-array conversion")))
}

fn read_string(bytes: &[u8], pos: &mut usize, field: &'static str) -> AndromedaResult<String> {
    let len = read_u32_le(bytes, pos, field)? as usize;
    let end = pos
        .checked_add(len)
        .ok_or_else(|| codec_error(format!("{field}: string position overflow")))?;
    let slice = bytes
        .get(*pos..end)
        .ok_or_else(|| codec_error(format!("{field}: string truncated")))?;
    *pos = end;
    String::from_utf8(slice.to_vec())
        .map_err(|_| codec_error(format!("{field}: string is not valid UTF-8")))
}

fn read_bool(bytes: &[u8], pos: &mut usize, field: &'static str) -> AndromedaResult<bool> {
    match read_u8(bytes, pos, field)? {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(codec_error(format!(
            "{field}: invalid boolean tag {other}; expected 0 or 1"
        ))),
    }
}

// ── Validation-state codec ────────────────────────────────────────────────────

fn encode_validation_state(buf: &mut Vec<u8>, state: &ForensicValidationState) {
    match state {
        ForensicValidationState::Pending => push_u8(buf, 0),
        ForensicValidationState::Validated => push_u8(buf, 1),
        ForensicValidationState::Failed(msg) => {
            push_u8(buf, 2);
            push_string(buf, msg);
        },
    }
}

fn decode_validation_state(
    bytes: &[u8],
    pos: &mut usize,
) -> AndromedaResult<ForensicValidationState> {
    let disc = read_u8(bytes, pos, "validation_state discriminant")?;
    match disc {
        0 => Ok(ForensicValidationState::Pending),
        1 => Ok(ForensicValidationState::Validated),
        2 => {
            let msg = read_string(bytes, pos, "validation_state_failed_message")?;
            Ok(ForensicValidationState::Failed(msg))
        },
        other => Err(codec_error(format!(
            "unknown validation_state discriminant {other}"
        ))),
    }
}

// ── MapRefreshState codec ─────────────────────────────────────────────────────

fn encode_map_refresh_state(buf: &mut Vec<u8>, state: ForensicMapRefreshState) {
    push_u8(
        buf,
        match state {
            ForensicMapRefreshState::Idle => 0,
            ForensicMapRefreshState::RefreshPending => 1,
            ForensicMapRefreshState::Stale => 2,
        },
    );
}

fn decode_map_refresh_state(
    bytes: &[u8],
    pos: &mut usize,
) -> AndromedaResult<ForensicMapRefreshState> {
    let disc = read_u8(bytes, pos, "map_refresh_state discriminant")?;
    match disc {
        0 => Ok(ForensicMapRefreshState::Idle),
        1 => Ok(ForensicMapRefreshState::RefreshPending),
        2 => Ok(ForensicMapRefreshState::Stale),
        other => Err(codec_error(format!(
            "unknown map_refresh_state discriminant {other}"
        ))),
    }
}

// ── Anomaly codec (reuses ForensicAnomalyReport / WalScanStop) ────────────────

fn encode_anomaly_kind(buf: &mut Vec<u8>, kind: &ForensicAnomalyKind) {
    match kind {
        ForensicAnomalyKind::LsnChainBreak { offset } => {
            push_u8(buf, 0);
            push_u64_le(buf, *offset);
        },
        ForensicAnomalyKind::RecoverableTailTruncation { offset } => {
            push_u8(buf, 1);
            push_u64_le(buf, *offset);
        },
        ForensicAnomalyKind::ChecksumFailure { offset } => {
            push_u8(buf, 2);
            push_u64_le(buf, *offset);
        },
        ForensicAnomalyKind::UnexpectedRecordType { offset } => {
            push_u8(buf, 3);
            push_u64_le(buf, *offset);
        },
        ForensicAnomalyKind::EmptyTail => {
            push_u8(buf, 4);
        },
    }
}

fn decode_anomaly_kind(bytes: &[u8], pos: &mut usize) -> AndromedaResult<ForensicAnomalyKind> {
    let disc = read_u8(bytes, pos, "anomaly_kind discriminant")?;
    match disc {
        0 => {
            let offset = read_u64_le(bytes, pos, "lsn_chain_break offset")?;
            Ok(ForensicAnomalyKind::LsnChainBreak { offset })
        },
        1 => {
            let offset = read_u64_le(bytes, pos, "recoverable_tail offset")?;
            Ok(ForensicAnomalyKind::RecoverableTailTruncation { offset })
        },
        2 => {
            let offset = read_u64_le(bytes, pos, "checksum_failure offset")?;
            Ok(ForensicAnomalyKind::ChecksumFailure { offset })
        },
        3 => {
            let offset = read_u64_le(bytes, pos, "unexpected_record_type offset")?;
            Ok(ForensicAnomalyKind::UnexpectedRecordType { offset })
        },
        4 => Ok(ForensicAnomalyKind::EmptyTail),
        other => Err(codec_error(format!(
            "unknown anomaly_kind discriminant {other}"
        ))),
    }
}

fn encode_anomaly_report(buf: &mut Vec<u8>, report: &ForensicAnomalyReport) {
    push_u32_le(buf, report.anomalies.len() as u32);
    for anomaly in &report.anomalies {
        encode_anomaly_kind(buf, &anomaly.kind);
        push_string(buf, &anomaly.detail);
    }
    match report.scan_stop {
        None => push_u8(buf, 0),
        Some(stop) => {
            push_u8(buf, 1);
            push_u64_le(buf, stop.offset as u64);
            push_u8(
                buf,
                match stop.reason {
                    WalScanStopReason::TruncatedHeader => 0,
                    WalScanStopReason::TruncatedRecord => 1,
                    WalScanStopReason::CorruptHeader => 2,
                    WalScanStopReason::CorruptRecord => 3,
                    WalScanStopReason::LsnGap => 4,
                    WalScanStopReason::DuplicateOrReorderedLsn => 5,
                    WalScanStopReason::PreviousLsnMismatch => 6,
                },
            );
        },
    }
}

fn decode_anomaly_report(bytes: &[u8], pos: &mut usize) -> AndromedaResult<ForensicAnomalyReport> {
    let count = read_u32_le(bytes, pos, "anomaly_count")? as usize;
    let mut anomalies = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = decode_anomaly_kind(bytes, pos)?;
        let detail = read_string(bytes, pos, "anomaly_detail")?;
        anomalies.push(ForensicAnomaly { kind, detail });
    }
    let has_stop = read_bool(bytes, pos, "has_scan_stop")?;
    let scan_stop = if has_stop {
        let raw_offset = read_u64_le(bytes, pos, "scan_stop_offset")?;
        let reason_tag = read_u8(bytes, pos, "scan_stop_reason")?;
        let reason = match reason_tag {
            0 => WalScanStopReason::TruncatedHeader,
            1 => WalScanStopReason::TruncatedRecord,
            2 => WalScanStopReason::CorruptHeader,
            3 => WalScanStopReason::CorruptRecord,
            4 => WalScanStopReason::LsnGap,
            5 => WalScanStopReason::DuplicateOrReorderedLsn,
            6 => WalScanStopReason::PreviousLsnMismatch,
            other => {
                return Err(codec_error(format!("unknown scan_stop_reason tag {other}")));
            },
        };
        Some(WalScanStop {
            offset: raw_offset as usize,
            reason,
        })
    } else {
        None
    };
    Ok(ForensicAnomalyReport {
        anomalies,
        scan_stop,
    })
}

// ── Section encoders / decoders ───────────────────────────────────────────────

fn encode_catalog_section(buf: &mut Vec<u8>, section: &ForensicCatalogSection) {
    push_u64_le(buf, section.catalog_version);
    push_u64_le(buf, section.object_count);
    encode_validation_state(buf, &section.validation_state);
}

fn decode_catalog_section(
    bytes: &[u8],
    pos: &mut usize,
) -> AndromedaResult<ForensicCatalogSection> {
    let catalog_version = read_u64_le(bytes, pos, "catalog_version")?;
    let object_count = read_u64_le(bytes, pos, "object_count")?;
    let validation_state = decode_validation_state(bytes, pos)?;
    Ok(ForensicCatalogSection {
        catalog_version,
        object_count,
        validation_state,
    })
}

fn encode_wal_section(buf: &mut Vec<u8>, section: &ForensicWalSection) {
    push_u64_le(buf, section.coverage_start_lsn);
    push_u64_le(buf, section.coverage_end_lsn);
    push_u8(buf, u8::from(section.scan_complete));
    match &section.anomaly {
        None => push_u8(buf, 0),
        Some(report) => {
            push_u8(buf, 1);
            encode_anomaly_report(buf, report);
        },
    }
}

fn decode_wal_section(bytes: &[u8], pos: &mut usize) -> AndromedaResult<ForensicWalSection> {
    let coverage_start_lsn = read_u64_le(bytes, pos, "coverage_start_lsn")?;
    let coverage_end_lsn = read_u64_le(bytes, pos, "coverage_end_lsn")?;
    let scan_complete = read_bool(bytes, pos, "scan_complete")?;
    let has_anomaly = read_bool(bytes, pos, "has_anomaly")?;
    let anomaly = if has_anomaly {
        Some(decode_anomaly_report(bytes, pos)?)
    } else {
        None
    };
    Ok(ForensicWalSection {
        coverage_start_lsn,
        coverage_end_lsn,
        anomaly,
        scan_complete,
    })
}

fn encode_snapshot_section(buf: &mut Vec<u8>, section: &ForensicSnapshotSection) {
    push_u64_le(buf, section.snapshot_id);
    push_bytes(buf, &section.snapshot_hash);
    push_u8(buf, u8::from(section.manifest_validated));
}

fn decode_snapshot_section(
    bytes: &[u8],
    pos: &mut usize,
) -> AndromedaResult<ForensicSnapshotSection> {
    let snapshot_id = read_u64_le(bytes, pos, "snapshot_id")?;
    let snapshot_hash: [u8; 32] = read_fixed(bytes, pos, "snapshot_hash")?;
    let manifest_validated = read_bool(bytes, pos, "manifest_validated")?;
    Ok(ForensicSnapshotSection {
        snapshot_id,
        snapshot_hash,
        manifest_validated,
    })
}

fn encode_index_section(buf: &mut Vec<u8>, section: &ForensicIndexSection) {
    push_u64_le(buf, section.segment_index_count);
    push_u8(buf, u8::from(section.segment_index_validated));
    push_u64_le(buf, section.last_segment_id);
}

fn decode_index_section(bytes: &[u8], pos: &mut usize) -> AndromedaResult<ForensicIndexSection> {
    let segment_index_count = read_u64_le(bytes, pos, "segment_index_count")?;
    let segment_index_validated = read_bool(bytes, pos, "segment_index_validated")?;
    let last_segment_id = read_u64_le(bytes, pos, "last_segment_id")?;
    Ok(ForensicIndexSection {
        segment_index_count,
        segment_index_validated,
        last_segment_id,
    })
}

fn encode_maps_section(buf: &mut Vec<u8>, section: &ForensicMapsSection) {
    push_u64_le(buf, section.map_count);
    encode_map_refresh_state(buf, section.refresh_plan_state);
    push_u64_le(buf, section.staleness_max_lsn_lag);
}

fn decode_maps_section(bytes: &[u8], pos: &mut usize) -> AndromedaResult<ForensicMapsSection> {
    let map_count = read_u64_le(bytes, pos, "map_count")?;
    let refresh_plan_state = decode_map_refresh_state(bytes, pos)?;
    let staleness_max_lsn_lag = read_u64_le(bytes, pos, "staleness_max_lsn_lag")?;
    Ok(ForensicMapsSection {
        map_count,
        refresh_plan_state,
        staleness_max_lsn_lag,
    })
}

fn encode_security_audit_section(buf: &mut Vec<u8>, section: &ForensicSecurityAuditSection) {
    push_u64_le(buf, section.last_audit_lsn);
    push_u8(buf, u8::from(section.retention_boundary_satisfied));
    push_u64_le(buf, section.audit_record_count);
}

fn decode_security_audit_section(
    bytes: &[u8],
    pos: &mut usize,
) -> AndromedaResult<ForensicSecurityAuditSection> {
    let last_audit_lsn = read_u64_le(bytes, pos, "last_audit_lsn")?;
    let retention_boundary_satisfied = read_bool(bytes, pos, "retention_boundary_satisfied")?;
    let audit_record_count = read_u64_le(bytes, pos, "audit_record_count")?;
    Ok(ForensicSecurityAuditSection {
        last_audit_lsn,
        retention_boundary_satisfied,
        audit_record_count,
    })
}

// ── Error helpers ─────────────────────────────────────────────────────────────

fn section_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

fn codec_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
