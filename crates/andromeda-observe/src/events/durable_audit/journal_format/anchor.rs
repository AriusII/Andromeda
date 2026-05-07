use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::fs::File;

use super::super::{
    DurableAuditFailureKind, DurableAuditRecordIdentity, DurableAuditSinkResult, checksum64,
    sink_failure,
};
use super::{
    DurableAuditJournalScan, GENESIS_CHAIN_CHECKSUM, JOURNAL_ANCHOR_PREFIX,
    fields::{parse_hex_checksum, parse_u64_field, parse_usize_field, strip_field},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DurableAuditJournalAnchor {
    first_record_lsn: u64,
    last_record_lsn: u64,
    record_count: usize,
    tail_chain_checksum: u64,
}

pub(in crate::events::durable_audit) fn write_journal_chain_anchor(
    path: &Path,
    identity: Option<DurableAuditRecordIdentity>,
    failure_kind: DurableAuditFailureKind,
    first_record_lsn: u64,
    last_record_lsn: u64,
    record_count: usize,
    tail_chain_checksum: u64,
) -> DurableAuditSinkResult<()> {
    let anchor = DurableAuditJournalAnchor {
        first_record_lsn,
        last_record_lsn,
        record_count,
        tail_chain_checksum,
    };
    validate_anchor_shape(anchor).map_err(|reason| sink_failure(failure_kind, identity, reason))?;
    let line = anchor_line(anchor);
    let anchor_path = journal_anchor_path(path);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&anchor_path)
        .map_err(|error| {
            sink_failure(
                failure_kind,
                identity,
                format!("failed to open durable audit chain anchor: {error}"),
            )
        })?;
    file.write_all(line.as_bytes()).map_err(|error| {
        sink_failure(
            failure_kind,
            identity,
            format!("failed to write durable audit chain anchor: {error}"),
        )
    })?;
    file.sync_all().map_err(|error| {
        sink_failure(
            failure_kind,
            identity,
            format!("failed to flush durable audit chain anchor: {error}"),
        )
    })?;
    sync_parent_directory(&anchor_path, failure_kind, identity)?;
    Ok(())
}

pub(in crate::events::durable_audit) fn remove_journal_chain_anchor(
    path: &Path,
    failure_kind: DurableAuditFailureKind,
) -> DurableAuditSinkResult<()> {
    let anchor_path = journal_anchor_path(path);
    match fs::remove_file(&anchor_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(sink_failure(
            failure_kind,
            None,
            format!("failed to remove durable audit chain anchor: {error}"),
        )),
    }
}

pub(super) fn validate_journal_anchor(
    path: &Path,
    scan: &DurableAuditJournalScan,
) -> DurableAuditSinkResult<()> {
    let anchor_path = journal_anchor_path(path);
    if !anchor_path.exists() {
        if scan.result.evidence.records_scanned == 0 {
            return Ok(());
        }
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor is required for non-empty durable audit journal",
        ));
    }

    if scan.result.evidence.records_scanned == 0 {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor exists for empty durable audit journal",
        ));
    }

    if scan.last_chain_checksum == GENESIS_CHAIN_CHECKSUM {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor cannot reference the genesis checksum for a non-empty journal",
        ));
    }

    if scan.first_record_lsn.is_none() || scan.last_record_lsn.is_none() {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor requires scanned LSN evidence",
        ));
    }

    if scan.result.evidence.tail_chain_checksum != scan.last_chain_checksum {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit replay evidence tail checksum mismatch",
        ));
    }

    if scan.result.evidence.first_scanned_lsn != scan.first_record_lsn
        || scan.result.evidence.last_scanned_lsn != scan.last_record_lsn
    {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit replay evidence scanned LSN mismatch",
        ));
    }

    if !scan.result.evidence.chain_anchor_present {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit replay evidence must report the persisted chain anchor",
        ));
    }

    let anchor_text = fs::read_to_string(&anchor_path).map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            format!("failed to read durable audit chain anchor: {error}"),
        )
    })?;
    if !anchor_text.ends_with('\n') {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor has a truncated tail: last record must be record-delimited",
        ));
    }
    let mut latest_anchor = None;
    for (line_index, line) in anchor_text.lines().enumerate() {
        latest_anchor = Some(parse_anchor_line(line).map_err(|reason| {
            sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                None,
                format!(
                    "durable audit chain anchor corruption at record {}: {reason}",
                    line_index + 1
                ),
            )
        })?);
    }
    let anchor = latest_anchor.ok_or_else(|| {
        sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor must contain at least one record",
        )
    })?;

    if scan.first_record_lsn != Some(anchor.first_record_lsn) {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor first record LSN mismatch",
        ));
    }
    if scan.last_record_lsn != Some(anchor.last_record_lsn) {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor last record LSN mismatch",
        ));
    }
    if scan.result.evidence.records_scanned != anchor.record_count {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor record count mismatch",
        ));
    }
    if scan.last_chain_checksum != anchor.tail_chain_checksum {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor tail checksum mismatch",
        ));
    }
    Ok(())
}

pub(super) fn journal_anchor_path(path: &Path) -> PathBuf {
    let mut anchor = path.as_os_str().to_os_string();
    anchor.push(".chain");
    PathBuf::from(anchor)
}

#[cfg(unix)]
fn sync_parent_directory(
    path: &Path,
    failure_kind: DurableAuditFailureKind,
    identity: Option<DurableAuditRecordIdentity>,
) -> DurableAuditSinkResult<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let directory = File::open(parent).map_err(|error| {
        sink_failure(
            failure_kind,
            identity,
            format!("failed to open durable audit parent directory for sync: {error}"),
        )
    })?;
    directory.sync_all().map_err(|error| {
        sink_failure(
            failure_kind,
            identity,
            format!("failed to sync durable audit parent directory: {error}"),
        )
    })
}

#[cfg(not(unix))]
fn sync_parent_directory(
    _path: &Path,
    _failure_kind: DurableAuditFailureKind,
    _identity: Option<DurableAuditRecordIdentity>,
) -> DurableAuditSinkResult<()> {
    Ok(())
}

fn anchor_line(anchor: DurableAuditJournalAnchor) -> String {
    let payload = anchor_payload(anchor);
    let checksum = checksum64(payload.as_bytes());
    format!("{payload}|checksum={checksum:016x}\n")
}

fn anchor_payload(anchor: DurableAuditJournalAnchor) -> String {
    format!(
        "{JOURNAL_ANCHOR_PREFIX}|first_record_lsn={}|last_record_lsn={}|record_count={}|tail_chain_checksum={:016x}",
        anchor.first_record_lsn,
        anchor.last_record_lsn,
        anchor.record_count,
        anchor.tail_chain_checksum,
    )
}

fn parse_anchor_line(line: &str) -> Result<DurableAuditJournalAnchor, String> {
    let (payload, checksum_field) = line
        .rsplit_once("|checksum=")
        .ok_or_else(|| "missing checksum field".to_string())?;
    if !payload.is_ascii() {
        return Err("durable audit chain anchor payload must be ASCII".to_string());
    }
    let expected_checksum = parse_hex_checksum(checksum_field, "checksum")?;
    let actual_checksum = checksum64(payload.as_bytes());
    if actual_checksum != expected_checksum {
        return Err("durable audit chain anchor checksum mismatch".to_string());
    }

    let fields: Vec<&str> = payload.split('|').collect();
    if fields.len() != 5 {
        return Err("unexpected durable audit chain anchor field count".to_string());
    }
    if fields[0] != JOURNAL_ANCHOR_PREFIX {
        return Err("unsupported durable audit chain anchor format version".to_string());
    }

    let anchor = DurableAuditJournalAnchor {
        first_record_lsn: parse_u64_field(fields[1], "first_record_lsn")?,
        last_record_lsn: parse_u64_field(fields[2], "last_record_lsn")?,
        record_count: parse_usize_field(fields[3], "record_count")?,
        tail_chain_checksum: parse_hex_checksum(
            strip_field(fields[4], "tail_chain_checksum")?,
            "tail_chain_checksum",
        )?,
    };
    validate_anchor_shape(anchor)?;
    Ok(anchor)
}

fn validate_anchor_shape(anchor: DurableAuditJournalAnchor) -> Result<(), String> {
    if anchor.first_record_lsn == 0 || anchor.last_record_lsn == 0 {
        return Err("durable audit chain anchor LSNs must be non-zero".to_string());
    }
    if anchor.first_record_lsn > anchor.last_record_lsn {
        return Err("durable audit chain anchor LSN range must be ordered".to_string());
    }
    if anchor.record_count == 0 {
        return Err("durable audit chain anchor record count must be non-zero".to_string());
    }
    if anchor.tail_chain_checksum == 0 {
        return Err("durable audit chain anchor tail checksum must be non-zero".to_string());
    }
    Ok(())
}
